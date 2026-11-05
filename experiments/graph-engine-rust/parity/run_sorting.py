"""One-shot, guarded driver. Running this invokes the reference and needs approval."""

import argparse
import hashlib
import json
import os
import subprocess
import threading
import time
from pathlib import Path


MAX_OUTPUT = 128 * 1024 * 1024


def sha(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(65536), b""):
            digest.update(block)
    return digest.hexdigest()


def guarded_files(binding, packet):
    paths = {}
    root = Path(binding["source_root"])
    actual = set()
    for path in root.rglob("*"):
        assert not path.is_symlink(), f"source symlink: {path}"
        if path.is_file():
            actual.add(path.relative_to(root).as_posix())
    expected = {entry["path"] for entry in binding["source"]}
    assert actual == expected, "complete source inventory changed"
    for entry in binding["source"]:
        paths[str(root / entry["path"])] = entry["sha256"]
    for entry in binding["frozen_inputs"]:
        paths[entry["source"]] = entry["sha256"]
    for entry in binding["runtime"]:
        paths[entry["path"]] = entry["sha256"]
    for name in ("binary", "target_binary"):
        entry = binding[name]
        paths[entry["path"]] = entry["sha256"]
    for name in ("requests", "manifest"):
        entry = packet[name]
        paths[entry["path"]] = entry["sha256"]
    for path, expected_hash in paths.items():
        assert sha(path) == expected_hash, f"guard mismatch: {path}"
    return paths


def run_child(arguments, *, input_bytes, directory, environment, prefix, timeout=90):
    # The retained stream is bounded while reading. Overflow kills the child and
    # preserves its first excess chunk separately, with explicit incomplete status.
    process = subprocess.Popen(arguments, stdin=subprocess.PIPE if input_bytes is not None
                               else subprocess.DEVNULL, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, cwd=directory, env=environment)
    started = time.monotonic()
    states = {name: {"observed": 0, "retained": 0, "overflow": False,
                     "eof": False, "read_error": None}
              for name in ("stdout", "stderr")}
    input_state = {"complete": input_bytes is None, "error": None}

    def terminate():
        try:
            process.kill()
        except OSError:
            pass  # A concurrent reader or timeout may already have killed it.

    def capture(name, stream):
        state = states[name]
        try:
            with (directory / f"{prefix}.{name}.bin").open("xb") as output:
                while True:
                    block = stream.read(65536)
                    if not block:
                        state["eof"] = True
                        break
                    state["observed"] += len(block)
                    remaining = MAX_OUTPUT - state["retained"]
                    output.write(block[:remaining])
                    state["retained"] += min(len(block), remaining)
                    if len(block) > remaining and not state["overflow"]:
                        state["overflow"] = True
                        excess = block[remaining:]
                        (directory / f"{prefix}.{name}.overflow.bin").write_bytes(excess)
                        state["overflow_sample_bytes"] = len(excess)
                        terminate()
        except OSError as failure:
            state["read_error"] = repr(failure)
            terminate()
        finally:
            stream.close()

    def supply_input():
        try:
            process.stdin.write(input_bytes)
            process.stdin.close()
            input_state["complete"] = True
        except OSError as failure:
            input_state["error"] = repr(failure)
            try:
                process.stdin.close()
            except OSError:
                pass

    readers = [threading.Thread(target=capture, args=(name, getattr(process, name)))
               for name in states]
    for reader in readers:
        reader.start()
    writer = None
    if input_bytes is not None:
        writer = threading.Thread(target=supply_input)
        writer.start()
    timed_out = False
    try:
        process.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        timed_out = True
        terminate()
        process.wait()
    for reader in readers:
        reader.join()
    if writer is not None:
        writer.join()
    for state in states.values():
        state["bytes_beyond_cap"] = state["observed"] - state["retained"]
        state["discarded_bytes"] = state["bytes_beyond_cap"] - state.get("overflow_sample_bytes", 0)
    complete = all(s["eof"] and not s["overflow"] and not s["read_error"]
                   for s in states.values())
    return {"argv": arguments, "exit": process.returncode, "timed_out": timed_out,
            "elapsed_seconds": time.monotonic() - started, "raw_complete": complete,
            "capture": states, "input": input_state,
            "stdout_sha256": sha(directory / f"{prefix}.stdout.bin"),
            "stderr_sha256": sha(directory / f"{prefix}.stderr.bin")}


def streams_equal(left, right):
    with left.open("rb") as a, right.open("rb") as b:
        while True:
            first, second = a.read(65536), b.read(65536)
            if first != second:
                return False
            if not first:
                return True


def exact(left, right):
    if type(left) is not type(right):
        return False
    if isinstance(left, dict):
        return left.keys() == right.keys() and all(exact(left[key], right[key]) for key in left)
    if isinstance(left, list):
        return len(left) == len(right) and all(exact(a, b) for a, b in zip(left, right))
    return left == right


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("packet", type=Path)
    parser.add_argument("--packet-sha256", required=True)
    args = parser.parse_args()
    assert sha(args.packet) == args.packet_sha256, "packet guard mismatch"
    packet = json.loads(args.packet.read_text(encoding="utf-8-sig"))
    directory = Path(packet["output"])
    assert not directory.exists(), "output exists; unknown outcomes must never be retried"
    assert sha(packet["binding"]["path"]) == packet["binding"]["sha256"]
    binding = json.loads(Path(packet["binding"]["path"]).read_text(encoding="utf-8-sig"))
    before = guarded_files(binding, packet)
    requests = Path(packet["requests"]["path"]).read_bytes()
    cases = [json.loads(line) for line in requests.splitlines()]
    assert len(cases) == packet["cases"] and packet["cases"] in (104, 770)
    assert len({case["id"] for case in cases}) == len(cases)
    assert all(len(case.get("keys", ())) <= 1024 for case in cases)
    directory.mkdir()  # Exclusive marker survives crashes and prevents duplicate calls.
    environment = dict(os.environ, PYTHONDONTWRITEBYTECODE="1", PYTHONHASHSEED="0")
    report = {"packet_sha256": args.packet_sha256, "binding_sha256": packet["binding"]["sha256"],
              "cases": len(cases), "status": "STARTED", "guards_before": before,
              "source_files": len(binding["source"]), "oracle_calls_max": packet["cases"],
              "children_max": 2, "retries": 0}
    (directory / "started.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    source = run_child(
        packet["reference_argv"], input_bytes=None, directory=directory,
        environment=environment, prefix="source")
    native = run_child(
        packet["native_argv"], input_bytes=requests, directory=directory,
        environment=environment, prefix="native")
    differences = []
    with (directory / "source.stdout.bin").open("rb") as source_stream, \
            (directory / "native.stdout.bin").open("rb") as native_stream:
        for index, case in enumerate(cases):
            source_line, native_line = source_stream.readline(), native_stream.readline()
            same_value = False
            if source_line and native_line:
                try:
                    same_value = exact(json.loads(source_line), json.loads(native_line))
                except (ValueError, UnicodeError):
                    pass
            if not same_value or source_line != native_line:
                differences.append({"index": index, "case": case["id"],
                                    "typed_values_equal": same_value,
                                    "raw_line_equal": source_line == native_line})
    with (directory / "source.stdout.bin").open("rb") as stream:
        source_responses = sum(1 for _ in stream)
    with (directory / "native.stdout.bin").open("rb") as stream:
        native_responses = sum(1 for _ in stream)
    after = guarded_files(binding, packet)
    assert before == after
    assert sha(args.packet) == args.packet_sha256
    assert sha(packet["binding"]["path"]) == packet["binding"]["sha256"]
    raw_equal = all(streams_equal(directory / f"source.{name}.bin",
                                  directory / f"native.{name}.bin")
                    for name in ("stdout", "stderr"))
    passed = (not differences and raw_equal and source["exit"] == native["exit"] == 0
              and source["raw_complete"] and native["raw_complete"]
              and not source["timed_out"] and not native["timed_out"]
              and source["input"]["complete"] and native["input"]["complete"]
              and source_responses == native_responses == len(cases))
    report.update(status="PASS" if passed else "FAIL", source=source, native=native,
                  source_responses=source_responses, native_responses=native_responses,
                  differences=differences, required_raw_streams_equal=raw_equal,
                  guards_after=after, normalization="none; no path exceptions in this pure API corpus")
    (directory / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"report": str(directory / "report.json"), "passed": passed,
                      "cases": len(cases), "differences": len(differences)}))
    raise SystemExit(0 if passed else 1)


if __name__ == "__main__":
    main()
