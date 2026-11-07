"""Compare artifact policy/canonicalization boundaries without workload-count claims."""

import argparse
import copy
import dataclasses
import json
import subprocess
import sys
from pathlib import Path

from bindings import capture
from comparison import exact_equal


def prepare(policy):
    requests = []
    for kind, config in policy["artifact_kinds"].items():
        for size in (-1, 0, 1, config["max_bytes"], config["max_bytes"] + 1, True, False, 0.0, float("nan"), None, "0", [], {}):
            requests.append(dict(op="artifact_enforce_size", kind=kind, value=size, policy=policy))
    malformed = (None, True, False, -1, 0, 1, 1.25, float("nan"), float("inf"), "-1", "１２", "1_024", "bad", [], {}, 2**1000)
    for kind in (None, True, 1, "missing", [], {}, "task_brief"):
        for operation in ("artifact_kind_policy", "artifact_size_limit", "artifact_enforce_size"):
            requests.append(dict(op=operation, kind=kind, value=0, policy=policy))
    for path in (("artifact_kinds",), ("limits",), ("artifact_kinds", "task_brief"), ("artifact_kinds", "task_brief", "max_bytes"), ("limits", "artifact_bytes")):
        for candidate in malformed:
            record = copy.deepcopy(policy)
            current = record
            for part in path[:-1]:
                current = current[part]
            current[path[-1]] = candidate
            requests.append(dict(op="artifact_size_limit", kind="task_brief", policy=record))
    relatives = ("docs/file.json", "docs", "docs/./file", "docs//file", "docs-other/file", "a/../b/file", "/absolute", "//double/path", "///triple/path", "", ".", "é/Σ", "a\nb/file")
    roots = ([], ["docs/"], ["docs"], ["docs-other/", "docs/"], [""], ["."], ["/"], ["//"], ["a/../b/"], "docs/", None, [None], [1], {"docs/": 1})
    for relative in relatives:
        for configured in roots:
            requests.append(dict(op="artifact_within_roots", value=relative, roots=configured))
    for relative in ("docs/file.json", "docs/.env", "docs/private.key", "docs/SECRET.txt", "src/ΟΣ/file.json", "docs/../file", "docs\\file.json", None, [], "", "docs/line\nfile.json"):
        for patterns in ([], ["**/.env"], ["**/*secret*"], ["**/*.key"], ["src/ος/*"], [""], [None], "x", None, {"docs/*": 0}):
            record = copy.deepcopy(policy)
            record["denied_patterns"] = patterns
            requests.append(dict(op="artifact_path_allowed", value=relative, policy=record))
    values = (None, True, -0.0, float("nan"), float("inf"), "Σ é", "\ud800", {"b": 1, "a": "é"}, [1, True, None])
    for identifier in ("artifact-id", "Σ é", "\ud800"):
        for value in values:
            requests.append(dict(op="ledger_artifact", identifier=identifier, kind="collection", value=value))
    for value in [chr(point) for point in range(256)] + ["", "single'", 'double"', "both'\"", "\\path\\", "Σ é", "a\ud800\udfff", "\u2028\u2029", "\U000e0001", "\U0010ffff", "😀"]:
        requests.append(dict(op="repr_string", value=value, value_points=[ord(point) for point in value]))
    return requests


def oracle(request, evidence):
    try:
        operation = request["op"]
        if operation == "repr_string":
            result = repr("".join(chr(point) for point in request["value_points"]))
        elif operation == "artifact_kind_policy":
            result = evidence._kind_policy(request["kind"], request["policy"])
        elif operation == "artifact_size_limit":
            result = evidence.artifact_size_limit(request["kind"], request["policy"])
        elif operation == "artifact_enforce_size":
            result = evidence.enforce_artifact_size(request["kind"], request["value"], request["policy"])
        elif operation == "artifact_within_roots":
            result = evidence._within_roots(request["value"], request["roots"])
        elif operation == "artifact_path_allowed":
            result = evidence.policy_path_allowed(request["value"], request["policy"])
        else:
            result = dataclasses.asdict(evidence.canonical_ledger_artifact(request["identifier"], request["kind"], request["value"]))
        return dict(ok=True, result=result)
    except Exception as error:
        detail = dict(kind=type(error).__name__)
        if hasattr(error, "field"):
            detail.update(field=error.field, code=error.code)
        else:
            detail["message"] = str(error)
        return dict(ok=False, error=detail)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--oracle", type=Path, required=True)
    parser.add_argument("--rust", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    sys.path.insert(0, str(args.oracle.resolve()))
    from graph_engine import config, evidence
    args.output.mkdir(parents=True, exist_ok=False)
    repository = args.output / "repository"
    (repository / "docs").mkdir(parents=True)
    requests = prepare(config._default_profile_policy(repository))
    encoded = b"".join((json.dumps(request, sort_keys=True) + "\n").encode("ascii") for request in requests)
    completed = subprocess.run([str(args.rust.resolve())], input=encoded, capture_output=True, timeout=120)
    (args.output / "requests.ndjson").write_bytes(encoded)
    (args.output / "stdout.ndjson").write_bytes(completed.stdout)
    (args.output / "stderr.bin").write_bytes(completed.stderr)
    actual = [json.loads(line) for line in completed.stdout.splitlines()]
    expected, failures = [], []
    for index, request in enumerate(requests):
        response = oracle(request, evidence)
        expected.append(response)
        observed = actual[index] if index < len(actual) else None
        if not exact_equal(response, observed):
            failures.append(dict(index=index, request=request, oracle=response, native=observed))
    (args.output / "oracle.ndjson").write_text("".join(json.dumps(response, sort_keys=True) + "\n" for response in expected), encoding="ascii")
    report = dict(scope="artifact pure boundaries only, no original test identity credit", requests=len(requests), responses=len(actual), native_exit=completed.returncode, failures=failures, bindings=capture(args.rust))
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="ascii")
    print(json.dumps(dict(requests=len(requests), responses=len(actual), native_exit=completed.returncode, failures=len(failures), first_failures=[dict(index=item["index"], oracle=item["oracle"], native=item["native"]) for item in failures[:8]])))
    return int(bool(failures) or completed.returncode != 0 or len(actual) != len(requests))


if __name__ == "__main__":
    raise SystemExit(main())
