# Kokoro CPU narration

Default new local narration to Kokoro-82M through OpenVINO on explicit CPU.
MOSS-TTS and Qwen3-TTS remain available when explicitly selected and installed.
Preserve approved narration and existing episode pipelines; an engine-default
change does not authorize regeneration. The production script/voice review gates
still apply. Installation smoke tests are fixtures, not approved episode takes.

## Select a voice without inventing approval

Kokoro uses a fixed catalog of embeddings and cannot condition on the approved
MOSS synthetic reference. Model-level praise does not establish which embedding
was auditioned. Resolve the actual voice/sample from authorized records or ask for
that single choice while continuing independent setup. Keep `voice: null` until
chosen; the runner then requires `--voice`. An `af_heart` installation test does
not make it the user's approved narrator. `af_bella` is another female audition
candidate in the [official voice inventory](https://huggingface.co/hexgrad/Kokoro-82M/blob/main/VOICES.md).
Listen to matched text/loudness before locking a voice for new production.

## Isolate runtime and assets

Inspect the actual host, existing model/runtime configuration and free local disk.
Reuse its configured model volume. Laptop paths are not desktop paths; do not put
weights on an output network share merely because a previous episode used it.
Use a new environment and local model directory. Do not overwrite an existing
environment, install drivers or change global PATH/security settings.

Within requested installation authority, use stable official PyPI wheels for a
supported Python. The Windows Python 3.12 stack tested for this workflow is
`openvino==2026.4.1`, `openvino-genai==2026.4.1.0`,
`openvino-tokenizers==2026.4.1.0`, `numpy==2.5.4`, `soundfile==0.14.0`.
Retain the installation report, full dependency lock and `pip check` receipt.
Verify official wheels for another host rather than building an unplanned source
distribution or mixing incompatible OpenVINO release families. See the
[GenAI package dependency contract](https://pypi.org/project/openvino-genai/2026.4.1.0/).

Use an inspected, pinned local IR such as
[OpenVINO/Kokoro-82M-int8-ov](https://huggingface.co/OpenVINO/Kokoro-82M-int8-ov),
revision `9c035d0bfab136f0c1c525a5841d3411d7220c66` for the tested setup.
Download only model/configuration, dictionary and voice data, without executing
remote model code or exporting with `--trust-remote-code`. The card labels the
conversion for evaluation; a download alone does not prove runtime quality.
Retain the original [hexgrad model card](https://huggingface.co/hexgrad/Kokoro-82M),
voice inventory, Apache 2.0 license/notices and dependency provenance. Inspect
terms for every asset; one model identifier does not audit the entire bundle.

Place an `asset-manifest.json` in the model root, recording repository, immutable
revision, license and `files` entries with model-root-relative `path` and `sha256`,
such as `config.json` and `voices/af_heart.bin`.
Cover the IR XML/weights, config, pronunciation data, selected voices and retained
license/provenance. The runner checks the manifest hashes before loading and
requires the selected embedding to be included. Preserve that manifest with the
runtime lock and model files. It is a reproducibility record, not a license audit.

English uses bundled pronunciation dictionaries. Unknown technical terms need a
verified phonemization path and listening. `espeak-ng` is a separate GPLv3
dependency; an OpenVINO English G2P alternative also needs its own local assets.
See the [official speech sample](https://openvinotoolkit.github.io/openvino.genai/docs/samples/python/speech_generation/).
Do not silently install either or assume dictionary presence proves acronym
pronunciation. Retain worker warnings and mark missing listening/G2P evidence.

## Configure and run

The optional [kokoro_narration.py](../scripts/kokoro_narration.py) reads
`~/.agents.local/system-design-video/narration.json`, or an explicit `--config`.
Create that file during authorized setup with verified absolute local `python`
and `model_dir` paths. Its remaining fields are:

```json
{
  "engine": "kokoro-openvino",
  "device": "CPU",
  "voice": null,
  "language": "en-us",
  "speed": 1.0,
  "threads": 2,
  "timeout_seconds": 120
}
```

The example omits the two host-specific paths; it is not a complete config.
The CPU-only runner rejects NPU, GPU and AUTO settings. One lock beside the config
prevents overlapping jobs using that config. A pre-existing lock needs owner/job
reconciliation before removal; do not blindly delete it after interruption.
Use the same installed config for serial production. Separate configs are not a
global scheduler and can run concurrently if invoked independently.

From the actual installed skill directory:

```powershell
& $NarrationPython scripts/kokoro_narration.py --text-file $ReviewedText --voice af_heart --output-dir $NewTakeDirectory
```

Replace the audition voice with the selected embedding. The output directory
must not exist. The parent launches the configured Python in a separate worker,
uses local model paths and process-scoped offline settings, requests one inference
stream and 1-4 threads, and
verifies worker-only affinity to at most that many logical processors. The
default deadline is 120 seconds, configurable up to 3600 for tested longer takes.
The deadline or Ctrl+C terminates and waits for the worker; it never starts an automatic retry
or another backend. Worker stdout/stderr go directly to retained files, including
on cancellation. Termination has an additional bounded wait of up to 15 seconds.
If termination cannot be confirmed, an unknown-outcome receipt
and the config lock remain to block retry. Failed/partial output remains for diagnosis. A crash
or uncertain worker must settle before any manual retry. No NPU experiment is
part of this default workflow.

The high-level pipeline accepts the CPU properties but does not expose effective
property readback. Record requested thread/stream settings separately from the
verified process affinity. Successful explicit CPU inference is the device
evidence; no AUTO/fallback selection is used. This bounds the narration worker,
not other system processes or GPU work. Unsupported process-affinity platforms
fail before inference. Alignment must select its device too.
Do not claim the NVIDIA GPU is idle from the TTS choice alone.

Use a roughly 10-second representative fixture first. Record host, CPU, actual
WAV duration, startup/generation timings, responsiveness and listening scope.
Do not crop speech to force the test length or import another host's RTF numbers.
The output includes raw segment WAVs, `native.wav`, a job record, worker logs and
`narration.json` with hashes, versions, voice/model identity and sample offsets.
Retain these; no output replaces an approved take.

The `supervision.json` receipt records worker wall time, deadline, outcome and
automatic retry count. Worker stdout is captured separately from that receipt.

## Pace and pauses

Start at speed 1.0. `--speed` changes an audition before audio approval; preserve
approved speech and retime visuals afterward. No fixed WPM rule or automatic
time stretching applies. Prefer one connected take for short samples.

For controlled sentence/paragraph pauses, put each complete reviewed passage in a
separate blank-line-delimited block and pass `--sentence-gap-ms 550`, or the chosen
gap. No automatic sentence splitter assumes punctuation around abbreviations.
The runner synthesizes intact passages, retains their original PCM, and inserts
the specified digital silence between them. This is added silence, not a promise
that the total audible gap equals that value: native leading/trailing silence
can remain. Review voice continuity and both joins, then align the final waveform.
See [pause guidance](local-narration.md#continuous-takes-and-sentence-pauses).

## Integrate the actual audio contract

Use the returned sample rate. The tested model produces 24 kHz mono PCM16;
MOSS's historical 48 kHz stereo output is not a Kokoro requirement. The spec
checker accepts measured PCM WAV rates/channels. Read the actual downstream
scripts before substituting an engine, especially fixed-rate/sample-index math,
stereo-only array operations, CUDA calls and episode-specific absolute paths.

Where a renderer/export requires 48 kHz stereo, write a separate conversion with
explicit rate conversion and unity mono duplication, for example FFmpeg's
`pan=stereo|c0=c0|c1=c0` with output `-ar 48000`. Preserve playback speed, duration
and raw WAV identity. Check both channels and waveform lags. Do not merely label
24 kHz samples as 48 kHz or reuse timestamps scaled by a guessed ratio.

Measure the exact channel layout that will be mastered. Unity stereo duplication
changes aggregate LUFS under `dual_mono=false`; it is not a gain defect.
Use measured two-pass normalization and encoded true-peak checks under
[audio and delivery](audio-and-delivery.md), with the actual destination target.
The default speech starting point there is -18 LUFS and at most -2 dBTP encoded.
Keep conversion, normalization and optional pause edits as separate artifacts.

Kokoro supplies no usable word-alignment attention in this runner. Use a separate
ASR/forced-alignment engine on the actual final WAV. A cached Whisper model can
resample mono to 16 kHz and produce word estimates; explicitly select CPU where
GPU reservation matters. Reconcile recognized text to canonical words, preserve
uncertain boundaries and renew cues after audio edits. MOSS synthesis/codec
alignment is not established by a successful Kokoro take. An episode aligner may
still require an adapter even when this isolated ASR test passes.

Before installation updates, back up affected installed skill files and local
configuration. Apply only scoped, reviewed changes while retaining edited
supporting scripts/assets. Keep a hash manifest and explicit rollback list.
Do not replace an installed skill folder wholesale or modify graph profiles.
