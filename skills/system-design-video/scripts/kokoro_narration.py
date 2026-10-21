#!/usr/bin/env python3
"""Generate local Kokoro PCM WAVs in a bounded, explicit CPU worker."""

from __future__ import annotations

import argparse
import hashlib
import importlib.metadata
import json
import math
import os
import platform
import re
import subprocess
import sys
import time
from pathlib import Path


DEFAULT_CONFIG = Path.home() / ".agents.local/system-design-video/narration.json"


def sha256(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify_assets(model_dir):
    root = Path(model_dir).resolve()
    manifest = json.loads((root / "asset-manifest.json").read_text(encoding="utf-8-sig"))
    entries = manifest.get("files", [])
    required = {"config.json", "openvino_model.xml", "openvino_model.bin"}
    if not required.issubset({item.get("path") for item in entries}):
        raise ValueError("Asset manifest must cover model configuration, IR and weights")
    for item in entries:
        path = (root / item["path"]).resolve()
        if not path.is_relative_to(root) or sha256(path) != item["sha256"]:
            raise ValueError(f"Model asset missing, changed or outside model root: {item['path']}")
    return manifest


def write_json(path, value):
    with Path(path).open("x", encoding="utf-8") as stream:
        json.dump(value, stream, indent=2, allow_nan=False)
        stream.write("\n")


def settings_from_file(path, voice=None, speed=None, gap_ms=None):
    config_path = Path(path).resolve()
    settings = json.loads(config_path.read_text(encoding="utf-8-sig"))
    if settings.get("engine") != "kokoro-openvino" or settings.get("device") != "CPU":
        raise ValueError("This runner requires engine kokoro-openvino and explicit device CPU")
    for key in ("python", "model_dir"):
        target = Path(settings[key])
        valid_kind = target.is_file() if key == "python" else target.is_dir()
        if not target.is_absolute() or not valid_kind:
            raise ValueError(f"Missing absolute local {key}: {target}")
    settings["voice"] = voice or settings.get("voice")
    if not isinstance(settings["voice"], str) or not re.fullmatch(r"[a-z]{2}_[a-z0-9]+", settings["voice"]):
        raise ValueError("Choose an explicit Kokoro voice with --voice or the local config; no voice approval is inferred")
    embedding = Path(settings["model_dir"]) / "voices" / f"{settings['voice']}.bin"
    if not embedding.is_file() or embedding.stat().st_size == 0:
        raise ValueError(f"Missing Kokoro voice embedding: {embedding}")
    settings["voice_file"] = str(embedding)
    settings["speed"] = speed if speed is not None else settings.get("speed", 1.0)
    if type(settings["speed"]) not in (int, float) or not math.isfinite(settings["speed"]) or not 0.5 <= settings["speed"] <= 2:
        raise ValueError("Speed must be finite and between 0.5 and 2.0")
    for name, default, lower, upper in (("threads", 2, 1, 4), ("timeout_seconds", 120, 1, 3600)):
        settings[name] = settings.get(name, default)
        if type(settings[name]) is not int or not lower <= settings[name] <= upper:
            raise ValueError(f"{name} must be an integer between {lower} and {upper}")
    settings["gap_ms"] = gap_ms
    if gap_ms is not None and (type(gap_ms) is not int or not 0 <= gap_ms <= 5000):
        raise ValueError("Sentence gap must be 0-5000 ms, or omitted for one connected take")
    if settings.get("language", "en-us") not in ("en-us", "en-gb"):
        raise ValueError("This runner currently supports the verified English dictionary path only")
    settings["language"] = settings.get("language", "en-us")
    settings["config_path"] = str(config_path)
    settings["config_sha256"] = sha256(config_path)
    return settings


def apply_cpu_affinity(limit):
    if os.name == "nt":
        import ctypes
        from ctypes import wintypes
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.GetCurrentProcess.restype = wintypes.HANDLE
        kernel.GetProcessAffinityMask.argtypes = [wintypes.HANDLE, ctypes.POINTER(ctypes.c_size_t), ctypes.POINTER(ctypes.c_size_t)]
        kernel.SetProcessAffinityMask.argtypes = [wintypes.HANDLE, ctypes.c_size_t]
        handle = kernel.GetCurrentProcess()
        allowed, system = ctypes.c_size_t(), ctypes.c_size_t()
        if not kernel.GetProcessAffinityMask(handle, ctypes.byref(allowed), ctypes.byref(system)):
            raise ctypes.WinError(ctypes.get_last_error())
        processors = [index for index in range(allowed.value.bit_length()) if allowed.value & (1 << index)]
        selected = processors[:limit]
        mask = sum(1 << index for index in selected)
        if not mask or not kernel.SetProcessAffinityMask(handle, mask):
            raise ctypes.WinError(ctypes.get_last_error())
        if not kernel.GetProcessAffinityMask(handle, ctypes.byref(allowed), ctypes.byref(system)) or allowed.value != mask:
            raise RuntimeError("Worker CPU affinity could not be verified")
        return {"logical_processors": selected, "mask": mask, "scope": "this worker process only"}
    if hasattr(os, "sched_setaffinity"):
        selected = sorted(os.sched_getaffinity(0))[:limit]
        os.sched_setaffinity(0, selected)
        if set(os.sched_getaffinity(0)) != set(selected):
            raise RuntimeError("Worker CPU affinity could not be verified")
        return {"logical_processors": selected, "scope": "this worker process only"}
    raise RuntimeError("Verified worker CPU affinity is unavailable on this platform")


def passages(text, gap_ms):
    if not text.strip() or len(text) > 10000:
        raise ValueError("Supply 1-10000 characters of reviewed narration")
    return [text.strip()] if gap_ms is None else [part.strip() for part in re.split(r"\n\s*\n", text) if part.strip()]


def synthesize(job):
    settings = job["settings"]
    affinity = apply_cpu_affinity(settings["threads"])
    assets = verify_assets(settings["model_dir"])
    voice_relative = f"voices/{settings['voice']}.bin"
    if voice_relative not in {item["path"] for item in assets["files"]}:
        raise ValueError("Selected voice must be covered by the asset manifest")
    import numpy as np
    import openvino as ov
    import openvino_genai as genai
    import soundfile as sf

    output = Path(job["output_dir"])
    began = time.perf_counter()
    pipeline = genai.Text2SpeechPipeline(settings["model_dir"], "CPU",
                                         INFERENCE_NUM_THREADS=settings["threads"], NUM_STREAMS=1)
    load_seconds = time.perf_counter() - began
    shape = tuple(pipeline.get_speaker_embedding_shape())
    values = np.fromfile(settings["voice_file"], dtype=np.float32)
    if values.size != math.prod(shape) or not np.isfinite(values).all():
        raise ValueError("Kokoro embedding size or values do not match this model")
    embedding = ov.Tensor(values.reshape(shape))
    clips, offsets, sample_rate, position = [], [], None, 0
    segment_dir = output / "segments"
    segment_dir.mkdir()
    generation_start = time.perf_counter()
    for index, text in enumerate(passages(job["text"], settings["gap_ms"]), 1):
        result = pipeline.generate(text, embedding, language=settings["language"], speed=settings["speed"])
        if len(result.speeches) != 1:
            raise ValueError("Expected one waveform per reviewed passage")
        rate = int(result.output_sample_rate)
        samples = np.asarray(result.speeches[0].data).reshape(-1)
        if rate <= 0 or not samples.size or not np.isfinite(samples).all() or np.max(np.abs(samples)) >= 1:
            raise ValueError("Invalid, empty, non-finite or clipped generated waveform")
        if sample_rate is not None and sample_rate != rate:
            raise ValueError("Sample rate changed between passages")
        sample_rate = rate
        segment = segment_dir / f"{index:03d}.wav"
        sf.write(segment, samples, rate, subtype="PCM_16")
        pcm, actual_rate = sf.read(segment, dtype="int16")
        if actual_rate != rate or pcm.ndim != 1:
            raise ValueError("Segment PCM contract mismatch")
        gap = round(rate * settings["gap_ms"] / 1000) if clips and settings["gap_ms"] is not None else 0
        if gap:
            clips.append(np.zeros(gap, dtype=np.int16))
            position += gap
        offsets.append({"file": segment.relative_to(output).as_posix(), "sha256": sha256(segment),
                        "start_sample": position, "samples": len(pcm), "inserted_gap_samples": gap})
        clips.append(pcm)
        position += len(pcm)
    generation_seconds = time.perf_counter() - generation_start
    native = output / "native.wav"
    sf.write(native, np.concatenate(clips), sample_rate, subtype="PCM_16")
    manifest = Path(settings["model_dir"]) / "asset-manifest.json"
    report = {"status": "generated", "engine": "kokoro-openvino", "host": platform.node(),
              "device_requested": "CPU", "execution_device": "CPU",
              "execution_evidence": "Successful explicit CPU Text2SpeechPipeline; no AUTO, GPU, NPU or fallback path",
              "pipeline_property_readback": "not exposed by Text2SpeechPipeline; constructor accepted CPU thread/stream properties",
              "cpu_affinity": affinity, "inference_threads_requested": settings["threads"], "streams_requested": 1,
              "cpu_name": str(ov.Core().get_property("CPU", "FULL_DEVICE_NAME")),
              "cpu_fallback_executed": False, "npu_attempted": False,
              "voice": settings["voice"], "voice_file": settings["voice_file"], "voice_sha256": sha256(settings["voice_file"]),
              "voice_approval": "not established by generation", "language": settings["language"], "speed": settings["speed"],
              "model_dir": settings["model_dir"], "asset_manifest_sha256": sha256(manifest) if manifest.is_file() else None,
              "model_repository": assets.get("repository"), "model_revision": assets.get("revision"),
              "model_license": assets.get("license"), "asset_hashes_verified": True,
              "config_sha256": settings["config_sha256"], "text_sha256": hashlib.sha256(job["text"].encode()).hexdigest(),
              "runner_sha256": sha256(__file__), "python": sys.version, "python_executable": sys.executable,
              "packages": {name: importlib.metadata.version(name) for name in ("openvino", "openvino-genai", "openvino-tokenizers", "numpy", "soundfile")},
              "wav": "native.wav", "wav_sha256": sha256(native), "sample_rate": sample_rate, "channels": 1, "sample_width_bytes": 2,
              "samples": position, "duration_seconds": position / sample_rate,
              "load_seconds": load_seconds, "generation_seconds": generation_seconds,
              "generation_rtf": generation_seconds / (position / sample_rate), "segments": offsets,
              "listening": "pending", "pronunciation": "pending listening; inspect worker logs for dictionary/G2P warnings",
              "alignment": "not generated; align the actual final WAV with a separate ASR/alignment engine"}
    write_json(output / "narration.json", report)
    return report


def run(settings, text, output_dir):
    passages(text, settings["gap_ms"])
    output = Path(output_dir).resolve()
    lock = Path(settings["config_path"]).with_suffix(".lock")
    with lock.open("x", encoding="utf-8") as stream:
        stream.write(json.dumps({"pid": os.getpid(), "output": str(output)}))
    settled = True
    try:
        output.mkdir(parents=True, exist_ok=False)
        job = {"settings": settings, "text": text, "output_dir": str(output)}
        write_json(output / "job.json", job)
        environment = os.environ.copy()
        environment.update(HF_HUB_OFFLINE="1", TRANSFORMERS_OFFLINE="1", CUDA_VISIBLE_DEVICES="", PYTHONDONTWRITEBYTECODE="1")
        command = [settings["python"], "-B", str(Path(__file__).resolve()), "--worker", str(output / "job.json")]
        began = time.perf_counter()
        with (output / "worker-stdout.log").open("xb") as stdout, (output / "worker-stderr.log").open("xb") as stderr:
            worker = None
            settled = False
            try:
                worker = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr, env=environment,
                                          creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                exit_code = worker.wait(timeout=settings["timeout_seconds"])
            except BaseException as error:
                if worker is None:
                    write_json(output / "supervision.json", {"worker_pid": None,
                               "worker_wall_seconds": time.perf_counter() - began,
                               "deadline_seconds": settings["timeout_seconds"], "outcome": "unknown_lock_retained",
                               "automatic_retries": 0, "launch_error": str(error),
                               "reconciliation": "Launch did not return a process handle; verify whether a worker started before removing the lock"})
                    raise RuntimeError("CPU worker launch outcome is uncertain; lock retained. Reconcile before retrying") from error
                try:
                    worker.kill()
                    exit_code = worker.wait(timeout=15)
                    settled = True
                except BaseException as stop_error:
                    write_json(output / "supervision.json", {"worker_pid": worker.pid,
                               "worker_wall_seconds": time.perf_counter() - began,
                               "deadline_seconds": settings["timeout_seconds"], "outcome": "unknown_lock_retained",
                               "automatic_retries": 0, "stop_error": str(stop_error)})
                    raise RuntimeError("CPU worker outcome is uncertain; lock retained. Reconcile that worker before retrying") from stop_error
                outcome = ("interrupted_killed_waited" if isinstance(error, KeyboardInterrupt) else
                           "timed_out_killed_waited" if isinstance(error, subprocess.TimeoutExpired) else "error_killed_waited")
                write_json(output / "supervision.json", {"worker_pid": worker.pid,
                           "worker_wall_seconds": time.perf_counter() - began, "deadline_seconds": settings["timeout_seconds"],
                           "exit_code": exit_code, "outcome": outcome, "automatic_retries": 0})
                if isinstance(error, KeyboardInterrupt):
                    raise
                raise RuntimeError("CPU worker stopped and was killed/waited; no retry was started") from error
            settled = True
            write_json(output / "supervision.json", {"worker_pid": worker.pid,
                       "worker_wall_seconds": time.perf_counter() - began, "deadline_seconds": settings["timeout_seconds"],
                       "exit_code": exit_code, "outcome": "exited", "automatic_retries": 0})
            if exit_code != 0:
                raise RuntimeError(f"CPU worker exited {exit_code}; inspect {output / 'worker-stderr.log'}")
        return json.loads((output / "narration.json").read_text(encoding="utf-8"))
    finally:
        if settled:
            lock.unlink()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, default=DEFAULT_CONFIG)
    parser.add_argument("--text-file", type=Path)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--voice")
    parser.add_argument("--speed", type=float)
    parser.add_argument("--sentence-gap-ms", type=int, help="Insert this silence between blank-line-separated reviewed passages")
    parser.add_argument("--worker", type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args()
    try:
        if args.worker:
            report = synthesize(json.loads(args.worker.read_text(encoding="utf-8")))
        else:
            if args.text_file is None or args.output_dir is None:
                parser.error("--text-file and --output-dir are required")
            settings = settings_from_file(args.config, args.voice, args.speed, args.sentence_gap_ms)
            report = run(settings, args.text_file.read_text(encoding="utf-8-sig"), args.output_dir)
        print(json.dumps(report, indent=2, allow_nan=False))
        return 0
    except KeyboardInterrupt:
        print(json.dumps({"status": "cancelled", "error": "CPU worker interrupted; inspect supervision receipt and logs", "npu_attempted": False}), file=sys.stderr)
        return 130
    except (OSError, ValueError, RuntimeError, KeyError) as error:
        print(json.dumps({"status": "error", "error": str(error), "npu_attempted": False}), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
