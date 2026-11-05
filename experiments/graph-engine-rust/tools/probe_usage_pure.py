"""Capture bounded, synthetic usage API results without opening any session source."""

import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import sys
from unittest.mock import patch


def counters(input_count=10, output_count=1):
    return {"input_tokens": input_count, "output_tokens": output_count,
            "total_tokens": input_count + output_count, "cached_input_tokens": 0,
            "reasoning_output_tokens": 0, "cache_write_tokens": 0}


def binding():
    return {
        "usage_schema_version": 1, "binding_id": "g2-" + "a" * 24,
        "source_id": hashlib.sha256(b"synthetic-session").hexdigest(),
        "adapter": "codex_jsonl_v1", "branch_id": None, "attempt_id": None,
        "role": "supervisor", "phase": "scoping", "generation": 0,
        "start_offset": 0, "offset": 0, "prefix_sha256": hashlib.sha256(b"").hexdigest(),
        "previous": None, "observed": False, "closed": False, "broken": False,
        "diagnostics": [], "metric_partial": [], "segments": [], "boundary_kind": "baseline",
    }


def line(kind, payload):
    return (json.dumps({"type": kind, "payload": payload}) + "\r\n").encode("ascii")


def sample(values, identity="synthetic-session", contexts=None):
    data = line("session_meta", {"id": identity})
    for context in contexts or [{"model": "gpt-6-astra", "effort": "medium"}]:
        data += line("turn_context", context)
    for current, last in values:
        data += line("event_msg", {"type": "token_count", "info": {
            "total_token_usage": current, "last_token_usage": last}})
    return data


def corpus(usage):
    requests = []

    def add(operation, **fields):
        requests.append({"op": operation, **fields})

    atoms = [None, False, True, -1, 0, 1, 1.0, -0.0, float("inf"), float("nan"),
             [], {}, "", "10", 1 << 63, (1 << 63) - 1]
    for value in atoms:
        for operation in ("integer", "counters", "source_counters", "context"):
            add(operation, value=value)
    for key in usage.METRICS:
        for value in atoms:
            changed = {**counters(), key: value}
            add("counters", value=changed)
            add("source_counters", value=changed)
    for value in atoms:
        add("source_counters", value={**counters(), "cache_write_input_tokens": value, "cache_write_tokens": 99})
    for aggregate in (False, True):
        for size in (0, 1, (1 << 63) - 1, 1 << 63, 1 << 100):
            add("counters", value=counters(size, 0), aggregate=aggregate)
    for model in sorted(usage.MODELS | {"untrusted-arbitrary-model", "primary-thread", "gpt-6.1-sol"}):
        for effort in (*sorted(usage.EFFORTS), "INVALID", None, [], 1):
            add("context", value={"model": model, "reasoning_effort": effort})
    for effort in atoms:
        add("context", value={"model": "gpt-6-astra", "effort": effort, "reasoning_effort": "high"})
    base = binding()
    for key in base:
        for value in atoms:
            add("validate_binding", value={**base, key: value})
        changed = copy.deepcopy(base)
        del changed[key]
        add("validate_binding", value=changed)
    for key in ("phase", "model", "effort", "totals"):
        for value in atoms:
            segment = {"phase": "scoping", "model": "gpt-6-astra", "effort": "medium", "totals": counters()}
            segment[key] = value
            add("validate_binding", value={**base, "segments": [segment]})
    for model in sorted(usage.MODELS):
        add("validate_binding", value={**base, "segments": [{
            "phase": "implementation", "model": model, "effort": "ultra", "totals": counters()}]})
    complete = sample([(counters(), counters())])
    for offset in (*atoms, -10000, -1, len(complete), len(complete) + 1, 1 << 100):
        add("source_checkpoint", data_hex=complete.hex(), offset=offset)
    saved = {"offset": 0, "source_id": base["source_id"], "prefix_sha256": base["prefix_sha256"]}
    for key in saved:
        for value in atoms:
            add("validate_checkpoint", data_hex=complete.hex(), value={**saved, key: value})
    variants = [b"", complete, complete[:-1], complete[:-2], complete + b"{}\n", complete + b"broken\n",
                complete + b"[" * 25 + b"]" * 25 + b"\n", complete.replace(b"\r\n", b"\r"),
                complete.replace(b"\r\n", b"\n"), sample([], identity="INVALID ID"),
                complete + line("session_meta", {"id": "different-session"}),
                sample([(counters(), counters())], contexts=[{"model": "gpt-6-astra", "effort": "medium"},
                                                              {"model": "gpt-6-astra", "effort": "high"}])]
    for value in atoms:
        variants.append(sample([(value, value)]))
    for data in variants:
        add("read_source", data_hex=data.hex())
        add("checkpoint", data_hex=data.hex())
    for limits in ({"max_records": 1}, {"chunk_bytes": 40}, {"failures": ["source_unsafe"]},
                   {"failures": ["source_changed"]}, {"failures": ["source_limit"]}):
        add("read_source", data_hex=complete.hex(), **limits)
    for raw in (b"null", b"true", b"0", b"-0.0", b"1.25", b"NaN", b"Infinity", b"[1,2]", b'"\\ud800"',
                b"[" * 24 + b"0" + b"]" * 24, b"[" * 25 + b"0" + b"]" * 25,
                b"{broken}", b"123456789012345678901234567890", b"9" * 4301, b"\xff"):
        add("bounded_json", data_hex=raw.hex())
    for current, last in [(counters(20, 2), counters(10, 1)), (counters(20, 2), counters(20, 2)),
                          (counters(5, 1), counters(5, 1)), (counters(10, 1), None)]:
        data = sample([(current, last)])
        for boundary in ("baseline", "phase", "continuation"):
            for closing in (False, True):
                for previous in (None, counters(10, 1)):
                    add("advance", data_hex=data.hex(), value={**base, "previous": previous},
                        boundary_kind=boundary, closing=closing)
    for closed in (False, True):
        for observed in (False, True):
            for missing in (False, True):
                for diagnostic in ([], ["counter_reset"]):
                    add("summary", values=[{**base, "closed": closed, "observed": observed,
                                            "diagnostics": diagnostic, "segments": [{
                                                "phase": "scoping", "model": "gpt-6-astra", "effort": "medium",
                                                "totals": counters(1 << 100, 1)}]}], missing=missing)
    if len(requests) > 2500:
        raise RuntimeError("authorized corpus bound exceeded")
    return requests


def source(usage, request):
    changes = {"_read_source": lambda _: (bytes.fromhex(request["data_hex"]), request.get("failures", []))}
    for key, source_key in (("max_records", "MAX_RECORDS"), ("chunk_bytes", "CHUNK_BYTES"), ("max_bytes", "MAX_BYTES")):
        if key in request:
            changes[source_key] = request[key]
    with patch.multiple(usage, **changes):
        return usage.read_source("synthetic-memory-only")


def evaluate(usage, request):
    operation = request["op"]
    if operation == "integer":
        return usage._integer(request["value"])
    if operation == "counters":
        return usage._counters(request["value"], aggregate=request.get("aggregate", False))
    if operation == "source_counters":
        return usage._source_counters(request["value"])
    if operation == "context":
        return usage._context(request["value"])
    if operation == "bounded_json":
        return usage._bounded_json(bytes.fromhex(request["data_hex"]))
    if operation == "validate_binding":
        return usage._validate_binding(request["value"])
    if operation == "summary":
        values = request["values"]
        return usage._summary(values, [segment for value in values for segment in value["segments"]], request["missing"])
    current = source(usage, request)
    if operation == "read_source":
        return {"source_id": current.source_id, "snapshots": current.snapshots, "diagnostics": current.diagnostics}
    if operation == "source_checkpoint":
        return current.checkpoint(request.get("offset"))
    if operation == "checkpoint":
        with patch.object(usage, "read_source", return_value=current):
            return usage.checkpoint("synthetic-memory-only")
    if operation == "validate_checkpoint":
        return usage.validate_checkpoint(current, request["value"])
    if operation == "advance":
        value = copy.deepcopy(request["value"])
        usage._advance(value, current, request["closing"], request["boundary_kind"])
        return value
    raise RuntimeError(operation)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    root = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve()
    output = args.output.resolve()
    output.relative_to(root)
    output.mkdir(parents=True, exist_ok=False)
    repo = Path(__file__).resolve().parents[3]
    sys.path.insert(0, str(repo / "skills/software-engineering-graph"))
    from graph_engine import usage
    requests = corpus(usage)
    records = []
    for index, request in enumerate(requests):
        try:
            value = evaluate(usage, request)
            response = {"ok": True, "value": value,
                        "stored_hex": json.dumps(value, ensure_ascii=True).encode("ascii").hex()}
        except Exception as error:
            response = {"ok": False, "type": type(error).__name__, "message": str(error)}
        records.append({"index": index, "request": request, "expected": response})
    payload = ("\n".join(json.dumps(record, ensure_ascii=True, sort_keys=True) for record in records) + "\n").encode("ascii")
    (output / "records.jsonl").write_bytes(payload)
    (output / "requests.jsonl").write_bytes(("\n".join(json.dumps(record["request"], ensure_ascii=True) for record in records) + "\n").encode("ascii"))
    report = {"requests": len(records), "records_sha256": hashlib.sha256(payload).hexdigest(),
              "oracle_python": sys.executable, "python_version": sys.version,
              "oracle_script": str(Path(usage.__file__).resolve()),
              "oracle_script_sha256": hashlib.sha256(Path(usage.__file__).read_bytes()).hexdigest(),
              "generator_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "scope": "in-process pure APIs; _read_source patched to explicit synthetic bytes; no filesystem source access"}
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"requests": len(records), "records_sha256": report["records_sha256"]}))


if __name__ == "__main__":
    main()
