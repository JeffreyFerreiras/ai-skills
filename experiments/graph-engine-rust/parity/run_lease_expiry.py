"""One-shot lease comparison. Reference execution requires the exact packet grant."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import run_sorting

STREAM_CAP = 2 * 1024 * 1024
TIMEOUT_SECONDS = 15


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("packet", type=Path)
    parser.add_argument("--packet-sha256", required=True)
    args = parser.parse_args()
    assert run_sorting.sha(args.packet) == args.packet_sha256
    packet = json.loads(args.packet.read_text(encoding="utf-8"))
    assert run_sorting.sha(packet["binding"]["path"]) == packet["binding"]["sha256"]
    assert run_sorting.sha(packet["preflight"]["path"]) == packet["preflight"]["sha256"]
    binding = json.loads(Path(packet["binding"]["path"]).read_text(encoding="utf-8"))
    output = Path(packet["output"])
    assert not output.exists(), "output exists; unknown outcome must never be retried"
    before = run_sorting.guarded_files(binding, packet)
    requests = Path(packet["requests"]["path"]).read_bytes()
    manifest = json.loads(Path(packet["manifest"]["path"]).read_text(encoding="utf-8"))
    lines = requests.splitlines(keepends=True)
    cases = [json.loads(line) for line in lines]
    assert 0 < len(cases) == packet["cases"] == manifest["cases"] <= 192
    assert len({case["id"] for case in cases}) == len(cases)
    assert [{"id": case["id"], "sha256": hashlib.sha256(line).hexdigest()} for case, line in zip(cases, lines)] == manifest["cases_sha256"]
    output.mkdir()  # Exclusive marker remains after all failures and interruptions.
    report = {"status": "STARTED", "cases": len(cases), "source_calls_max": len(cases),
              "children_max": 2, "retries": 0, "guards_before": before,
              "packet_sha256": args.packet_sha256, "stream_cap": STREAM_CAP, "timeout_seconds": TIMEOUT_SECONDS}
    (output / "started.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    environment = dict(os.environ, PYTHONDONTWRITEBYTECODE="1", PYTHONHASHSEED="0")
    run_sorting.MAX_OUTPUT = STREAM_CAP
    source = run_sorting.run_child(packet["reference_argv"], input_bytes=None, directory=output, environment=environment, prefix="source", timeout=TIMEOUT_SECONDS)
    native = run_sorting.run_child(packet["native_argv"], input_bytes=requests, directory=output, environment=environment, prefix="native", timeout=TIMEOUT_SECONDS)
    source_lines = (output / "source.stdout.bin").read_bytes().splitlines(keepends=True)
    native_lines = (output / "native.stdout.bin").read_bytes().splitlines(keepends=True)
    differences = []
    for index, case in enumerate(cases):
        left = source_lines[index] if index < len(source_lines) else b""
        right = native_lines[index] if index < len(native_lines) else b""
        if left != right:
            fields = []
            try:
                a, b = json.loads(left), json.loads(right)
                fields = [key for key in ("id", "clock_reads", "outcome") if not run_sorting.exact(a.get(key), b.get(key))]
            except (ValueError, UnicodeError):
                fields = ["protocol"]
            differences.append({"index": index, "id": case["id"], "fields": fields, "raw_equal": False})
    after = run_sorting.guarded_files(binding, packet)
    assert before == after and run_sorting.sha(args.packet) == args.packet_sha256
    assert run_sorting.sha(packet["binding"]["path"]) == packet["binding"]["sha256"]
    raw_equal = all(run_sorting.streams_equal(output / f"source.{stream}.bin", output / f"native.{stream}.bin") for stream in ("stdout", "stderr"))
    passed = not differences and raw_equal and len(source_lines) == len(native_lines) == len(cases) and all(child["exit"] == 0 and not child["timed_out"] and child["raw_complete"] and child["input"]["complete"] for child in (source, native))
    report.update(status="PASS" if passed else "FAIL", source=source, native=native,
                  guards_after=after, differences=differences, required_raw_streams_equal=raw_equal,
                  source_records=len(source_lines), native_records=len(native_lines), normalization="none")
    path = output / "report.json"
    path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"report": str(path), "sha256": run_sorting.sha(path), "passed": passed, "differences": len(differences)}))
    raise SystemExit(0 if passed else 1)


if __name__ == "__main__":
    main()
