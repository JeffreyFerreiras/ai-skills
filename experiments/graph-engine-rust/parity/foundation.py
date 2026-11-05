"""Differential checks for the foundation; this is not full-engine parity."""

import argparse
import base64
import hashlib
import json
import math
import random
import struct
import subprocess
import sys
from pathlib import Path
from bindings import capture
from comparison import exact_equal


def oracle(request, contracts, ids, merge, constraints):
    op, value = request["op"], request.get("value")
    try:
        if op == "canonical":
            result = ids.canonical_bytes(value).decode("utf-8")
        elif op in {"cli_json", "stored_json", "sorted_stored_json"}:
            result = json.dumps(value, sort_keys=op != "stored_json",
                                separators=(",", ":") if op == "cli_json" else None)
        elif op == "parse":
            result = json.dumps(json.loads(value), sort_keys=True, separators=(",", ":"))
        elif op in {"lower", "casefold"}:
            result = getattr(value, op)()
        elif op == "normcase":
            result = value.replace("/", "\\").lower() if request.get("windows") else value
        elif op == "py_eq":
            result = value == request["other"]
        elif op == "repository_digest":
            result = ids.repository_digest(value)
        elif op == "stable_id":
            result = ids.stable_id(*(request[key] for key in ("run_id", "policy_digest", "entity_type", "template_key", "generation")), request.get("specialist_tag"))
        elif op == "merge_readiness":
            result = merge.assess_merge_readiness(**value)
        elif op == "sqlite_schema":
            import sqlite3
            from graph_engine.state import SCHEMA
            with sqlite3.connect(":memory:") as connection:
                connection.executescript(SCHEMA)
                result = [list(row) for row in connection.execute("SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY type,name")]
        elif op == "constraints_validate":
            result = constraints.validate_bundle(value)
        elif op == "constraints_select":
            normalize = (lambda path: path.replace("/", "\\").lower()) if request.get("windows") else (lambda path: path)
            result = constraints.select_constraints(value, request["context"], activated=request.get("activated", False), supported=request.get("supported", ()), authority_refs=request.get("authority_refs", ()), normalize=normalize)
        elif op in {"snapshot", "json_snapshot"}:
            function = contracts.safe_json_snapshot if op == "json_snapshot" else contracts.safe_file_snapshot
            snapshot = function(Path(request["path"]), list(map(Path, request["roots"])), request["maximum"])
            result = dict(path=str(snapshot.path), data=base64.b64encode(snapshot.data).decode("ascii"), digest=snapshot.digest, size=snapshot.size, parsed=snapshot.parsed, device=snapshot.identity[0], inode=snapshot.identity[1])
        elif op == "bounded_string":
            result = contracts.bounded_string(value, "f", request.get("maximum", 4096))
        elif op == "lexical_relative":
            result = contracts.lexical_relative(value, "ref")
        elif op == "validate_ref":
            result = contracts.validate_ref(value, "f", request.get("content_required", False))
        elif op == "reject_sensitive":
            result = contracts.reject_sensitive(value)
        else:
            result = getattr(contracts, op)(value, "f")
        return {"ok": True, "result": result}
    except contracts.ContractError as error:
        return {"ok": False, "error": {"kind": "ContractError", "field": error.field, "code": error.code}}
    except OSError as error:
        return {"ok": False, "error": {"kind": type(error).__name__}}
    except TypeError as error:
        return {"ok": False, "error": {"kind": "TypeError", "message": str(error)}}
    except (ValueError, UnicodeError, RecursionError) as error:
        return {"ok": False, "error": {"kind": type(error).__name__,
            "message": error.msg if isinstance(error, json.JSONDecodeError) else str(error),
            "position": getattr(error, "pos", getattr(error, "start", 0))}}


def cases(fixtures):
    values = [None, True, False, 0, -1, 2**400, -2**400, 0.0, -0.0,
              float("nan"), float("inf"), -float("inf"), "é😀\x7f",
              "\ud800", "\udfff", "\ud800\ud800", {"z": 1, "a": -0.0},
              {"\ud800": "bad"}, {"é": "\ud800"}, [1, "\udfff", 3]]
    randomizer = random.Random(431782)
    values += [struct.unpack("!d", randomizer.getrandbits(64).to_bytes(8, "big"))[0]
               for _ in range(3000)]
    result = [{"op": op, "value": value} for op in ("canonical", "cli_json", "stored_json", "sorted_stored_json") for value in values]
    result.append({"op": "sqlite_schema"})
    for value, other in [(float("nan"), float("nan")), ([float("nan")], [float("nan")]), ({"a": float("nan")}, {"a": float("nan")}), (2**53+1, float(2**53+1)), (True, 1), (False, -0.0)]:
        result.append({"op": "py_eq", "value": value, "other": other})
    for text in ["ΟΣ", "ΟΣΑ", "AΣ\u0345", "AΣ\u0345A", "İΣ", "Σ", "Straße",
                 "AΣ'", "AΣ'A", "AΣ:\u0345A", "A/ΟΣ", "\ud800Σ", "AΣ\u200dA"]:
        result += [{"op": op, "value": text} for op in ("lower", "casefold")]
        result += [{"op": "normcase", "value": text, "windows": windows} for windows in (True, False)]
    for _ in range(400):
        text = "".join(chr(randomizer.randrange(0x110000)) for _ in range(12)) + "AΣ\u0345AΣ"
        result += [{"op": op, "value": text} for op in ("lower", "casefold")]
    for text in ["", "null", "true", "NaN", "Infinity", "-Infinity", "01", "-01",
                 "1e9999", "1e-9999", "1e+", "1.", "-", "[]", "{}", "[1,]",
                 '{"a":1,"a":2}', '{"a" 1}', '{"a":}', '{a:1}', "[",
                 '"abc', '"a\x01b"', '"\\q"', '"\\uXYZ0"',
                 '"\\ud800\\uDC00"', '"\\ud800\\u1234"', '"\ud800"', '"\udfff"', "\ud800", "\ufeff[]", "\u00a0null",
                 "["*24+"0"+"]"*24, "["*400+"0"+"]"*400,
                 "9"*4300, "9"*4301]:
        result.append({"op": "parse", "value": text})
    for depth in (900, 990, 993, 994, 995, 998, 1000, 2000, 2996, 2997, 2998, 2999, 3000, 10000):
        result.append({"op": "parse", "value": "["*depth+"0"+"]"*depth})
        result.append({"op": "parse", "value": '{"a":'*depth+"0"+"}"*depth})
    for value in [None, False, 1, [], {}, "", "-bad", "OK-1", "ä", "x"*64, "x"*65]:
        result.append({"op": "opaque", "value": value})
    for value in [None, "a"*64, "A"*64, "f"*63, "g"*64]:
        result.append({"op": "digest", "value": value})
    for value in ["hello", "", "a\x01b", "bearer İ", "bearer ß", "bearer ﬀ", "BEARER\u0085abc", "bearer\u001cabc", "-----BEGIN FOO PRIVATE KEY-----", "eyJx.eyJy.z", "eyJ.eyJy.z", "\ud800"]:
        result.append({"op": "bounded_string", "value": value})
    for value in [{"Access-Key": "hidden"}, {"nested": [{"secret": "hidden"}]}, {"\ud800": {"password": "hidden"}}, {"nested": "bearer İ"}, {"ordinary": "allowed"}, [], 1e300]:
        result.append({"op": "reject_sensitive", "value": value})
    for value in ["a", "src/a.py", "src/", "src\\a.py", "/abs", "C:/win", "a/../b", "a//b", "a/.git/x", "a/.env", ".pem", "key.pem", "src/\ud800.txt", "a///"]:
        result.append({"op": "lexical_relative", "value": value})
    for value in ["repo:a.json", "runtime:artifacts/a.json", "runtime:other/a.json", "profile:software-engineering-graph/references/a.json", "profile:software-engineering-graph/bad/a.json", "ledger:id", "thread:t1", "authority:a#b", "http://example", "FILE:local", "repo:a#sha256="+"a"*64, "repo:src/\ud800.json"]:
        result += [{"op": "validate_ref", "value": value, "content_required": content} for content in (True, False)]
    sha = "a"*40
    base = dict(head_sha=sha, reviewed_sha=sha, ci_head_sha=sha,
                required_check_ids=["z", "a"], checks={"z": "success", "a": "success"},
                review_decision="APPROVED", unresolved_review_count=0, graph_status="complete")
    result.append({"op": "merge_readiness", "value": base})
    for changes in [{"head_sha": "bad"}, {"reviewed_sha": "b"*40}, {"ci_head_sha": "c"*40}, {"required_check_ids": []}, {"required_check_ids": ["a", "a"]}, {"checks": {"a": "failure"}}, {"unresolved_review_count": False}, {"dry_run": 1}]:
        result.append({"op": "merge_readiness", "value": dict(base, **changes)})
    record = dict(id="C-1", revision=1, statement="Use a bounded rule", rationale="Retain its proof", state="active", repository_id="REPO-1", paths=["src/"], roles=["senior_engineer"], acceptance_ids=["AC-1"], finding_id="F-1", confirmed_finding_ref="ledger:finding#sha256="+"a"*64, accepted_fix_ref="ledger:fix#sha256="+"b"*64, authority_ref="authority:approved", invalidation_reason=None, supersedes=[])
    context = dict(repository_id="REPO-1", acceptance_ids=["AC-1"], role="senior_engineer", scope_paths=["src/a.py"])
    import copy
    for changes in [{}, {"revision": True}, {"revision": 0}, {"revision": 1000001}, {"paths": ["src\\a.py"]}, {"paths": ["src/*.py"]}, {"roles": ["x", "x"]}, {"state": "unknown"}, {"state": "invalidated"}, {"supersedes": ["C-1:1"]}]:
        result.append({"op": "constraints_validate", "value": {"kind": "planning_constraints", "schema_version": 1, "records": [dict(record, **changes)]}})
    for changes in [{"state": []}, {"state": {}}, {"roles": [[]]}, {"acceptance_ids": [{}]}, {"roles": [float("nan"), float("nan")]}, {"roles": [True, 1]}, {"roles": [2**100, float(2**100)]}]:
        result.append({"op": "constraints_validate", "value": {"kind": "planning_constraints", "schema_version": 1, "records": [dict(record, **changes)]}})
    base_bundle = {"kind": "planning_constraints", "schema_version": 1, "records": [record]}
    for changes in [{}, {"state": "candidate"}, {"state": "superseded"}, {"repository_id": "OTHER"}, {"roles": ["other"]}, {"acceptance_ids": ["OTHER"]}, {"paths": ["docs/"]}, {"authority_ref": "authority:other"}]:
        bundle = copy.deepcopy(base_bundle)
        bundle["records"][0].update(changes)
        for activated in (True, False):
            for support in ([], [record["confirmed_finding_ref"], record["accepted_fix_ref"]]):
                result.append({"op": "constraints_select", "value": bundle, "context": context, "activated": activated, "supported": support, "authority_refs": ["authority:approved"]})
    duplicate = copy.deepcopy(record)
    duplicate["revision"] = 2
    result.append({"op": "constraints_select", "value": dict(base_bundle, records=[record, duplicate]), "context": context})
    for repository in ["REPO-1", "é-😀", "C:\\Work\\ΟΣ"]:
        result.append({"op": "repository_digest", "value": repository})
    for generation in [0, 1, 99, -1]:
        for tag in [None, "", "security_privacy", "é"]:
            result.append(dict(op="stable_id", run_id="RUN-1", policy_digest="a"*64, entity_type="branch", template_key="senior_engineer", generation=generation, specialist_tag=tag))
    root = fixtures / "repo with é and spaces"
    root.mkdir(parents=True)
    (root / "src").mkdir()
    for name, data in [("file.bin", b"a\x00b\xff"), ("é.json", b'{"value":"ok"}'), ("bad.json", b'{"value":}'), ("invalid-utf8.json", b'"\xff"'), ("secret.json", b'{"password":"hidden"}')]:
        path = root / "src" / name
        path.write_bytes(data)
        for op in ("snapshot", "json_snapshot"):
            result.append(dict(op=op, path=str(path), roots=[str(root)], maximum=4096))
    result += [dict(op="snapshot", path=str(root / "src/file.bin"), roots=[str(root)], maximum=3), dict(op="snapshot", path=str(root / "src/file.bin"), roots=[str(root / "other")], maximum=4096), dict(op="snapshot", path=str(root / "src/missing.bin"), roots=[str(root)], maximum=4096), dict(op="snapshot", path=str(root / "src"), roots=[str(root)], maximum=4096)]
    if sys.platform == "win32":
        result.append(dict(op="snapshot", path=str(root / "src/file.bin").replace("\\", "/"), roots=[str(root).replace("\\", "/")], maximum=4096))
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--oracle", type=Path, required=True)
    parser.add_argument("--rust", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    sys.path.insert(0, str(args.oracle.resolve()))
    from graph_engine import contracts, ids, merge_readiness, constraints
    args.output.mkdir(parents=True, exist_ok=True)
    requests = cases(args.output / "fixtures")
    # The private protocol is JSON. Match CPython's JSON constant identities,
    # including its shared NaN instance, before evaluating either adapter.
    requests = [json.loads(json.dumps(request, ensure_ascii=True)) for request in requests]
    encoded = b"".join((json.dumps(request, ensure_ascii=True, separators=(",", ":")) + "\n").encode("ascii") for request in requests)
    completed = subprocess.run([str(args.rust.resolve())], input=encoded, capture_output=True, timeout=120)
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "requests.ndjson").write_bytes(encoded)
    (args.output / "stdout.ndjson").write_bytes(completed.stdout)
    (args.output / "stderr.bin").write_bytes(completed.stderr)
    actual = [json.loads(line) for line in completed.stdout.splitlines()]
    failures, oracle_lines = [], []
    for index, request in enumerate(requests):
        expected = oracle(request, contracts, ids, merge_readiness, constraints)
        oracle_lines.append(json.dumps(expected, ensure_ascii=True, separators=(",", ":")))
        if index >= len(actual) or not exact_equal(actual[index], expected):
            failures.append({"index": index, "request": request, "oracle": expected,
                             "native": actual[index] if index < len(actual) else None})
    (args.output / "oracle.ndjson").write_text("\n".join(oracle_lines)+"\n", encoding="ascii")
    report = {"scope": "foundation only; not full-engine parity", "requests": len(requests),
              "responses": len(actual), "native_exit": completed.returncode,
              "failures": failures, "binary_sha256": hashlib.sha256(args.rust.read_bytes()).hexdigest()}
    report["bindings"] = capture(args.rust)
    (args.output / "report.json").write_text(json.dumps(report, ensure_ascii=True, indent=2)+"\n", encoding="ascii")
    first_failures = [{"index": failure["index"], "op": failure["request"]["op"],
                       "value_length": len(failure["request"].get("value", "")) if isinstance(failure["request"].get("value"), str) else None,
                       "oracle_error": failure["oracle"].get("error"),
                       "native_error": failure["native"].get("error") if failure["native"] else "MISSING_RESPONSE"}
                      for failure in failures[:8]]
    print(json.dumps({"requests": len(requests), "responses": len(actual), "native_exit": completed.returncode, "failures": len(failures), "first_failures": first_failures}, ensure_ascii=True))
    return int(bool(failures) or completed.returncode != 0 or len(actual) != len(requests))


if __name__ == "__main__":
    raise SystemExit(main())
