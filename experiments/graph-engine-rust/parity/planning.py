"""Targeted deterministic-planning differential checks, separate from full parity."""

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path
from bindings import capture
from comparison import exact_equal


def cases(hosts, fixtures):
    result = []
    models = set(hosts.MODEL_OPTIONS["codex"]) | set(hosts.MODEL_OPTIONS["claude"]) | set(hosts.MODEL_OPTIONS["cursor"]) | {hosts.MUSE_MODEL, "primary-thread", "unknown"}
    for host in hosts.known_hosts():
        for revision in [None, 0, 1, *hosts.SUPPORTED_CATALOG_REVISIONS[host], 11]:
            result.append(dict(op="hosts", host=host, function="model_options", revision=revision))
            for workload in ("helper", "review", "implementation"):
                result.append(dict(op="hosts", host=host, function="recommended_assignment", revision=revision, workload=workload))
            for model in sorted(models):
                for effort in ("low", "medium", "high", "xhigh", "max", "ultra", "minimal", "inherited"):
                    result.append(dict(op="hosts", host=host, function="selected_dispatch_model", revision=revision, model=model, effort=effort))
        for model in sorted(models):
            for effort in ("low", "medium", "high", "xhigh", "max", "ultra", "minimal", "inherited"):
                for historical in (True, False):
                    result.append(dict(op="hosts", host=host, function="dispatch_weight_for", model=model, effort=effort, historical=historical))
        result.append(dict(op="hosts", host=host, function="supported_dispatch_weights"))
        for cls in ("economy", "reasoning", "primary-thread", "unknown"):
            for effort in ("low", "medium", "high", "xhigh", "max", "inherited", "none"):
                result.append(dict(op="hosts", host=host, function="resolve_assignment", **{"class": cls}, effort=effort))
    for host in hosts.known_hosts():
        for schema in (1, 2, 3):
            task = dict(schema_version=schema, task_id="TASK-1", request_mode="delivery", minimum_route="full_delivery", mandatory_impact_tags=[], risk_level="low")
            if schema >= 2:
                task["model_sizing"] = dict(scope_extent="bounded", uncertainty="low")
            if schema == 3:
                task["helper_allowance"] = dict(ref="repo:docs/helper.json", sha256="a"*64)
            for size in (None, "small", "medium", "large", "invalid"):
                result.append(dict(op="execution", function="build", value=task, host=host, size=size))
                for revision in [None, *hosts.SUPPORTED_CATALOG_REVISIONS[host]]:
                    stored = dict(host=host)
                    if revision is not None:
                        stored["catalog_revision"] = revision
                    result.append(dict(op="execution", function="reconstruct", value=task, stored=stored, size=size))
            for key in ("catalog_revision", "code_review_panel_version", "simplification_review_version", "capability_projection_version"):
                for marker in (None, False, 0, 2, 10.0, "1", [], {}):
                    result.append(dict(op="execution", function="reconstruct", value=task, stored=dict(host=host, **{key: marker})))
    import random
    randomizer = random.Random(51727)
    patterns = ["", ".", "/", "//", "*", "**", "?", "??", "/*", "//*", "[!/]", "[/]", "[\n]", "[!\n]", "**/private.txt", "*.key", "a?c", "[a-z]", "[!a-z]", "[]-a]", "[z-a]", "[!z-a]", "[a--b]", "[a-b-c]", "[!]]", "[", "[]", "[!]"]
    texts = ["", ".", "/", "//", "///", "/a", "//a", "///a", "a", "b", "-", "]", "src/private.txt", "key.key", "a/b/c", "a//b", "a\nb", "\n", "é", "\ud800"]
    for _ in range(600):
        patterns.append("".join(randomizer.choice("abc-*?[]!/") for _ in range(randomizer.randrange(1, 12))))
    for pattern in patterns:
        for text in texts:
            for posix in (False, True):
                result.append(dict(op="glob", value=text, pattern=pattern, posix_path=posix))
    for target in ["repo:src/a.py", "repo:src/private.txt", "repo:src\\a.py", "repo:src/../a.py", "repo:src/file.py.", "runtime:artifacts/a.json", "profile:software-engineering-graph/references/a.json"]:
        for denied in [[], ["**/private.txt"], [""], ["."]]:
            result.append(dict(op="filesystem_capability_target", value=target, denied=denied))
    import copy
    base = dict(effect="filesystem_write", action="edit", target_ref="repo:src/feature/")
    for target in ["repo:src/feature/a.py", "repo:SRC/FEATURE/a.py", "repo:src/", "repo:src/feature2/a.py", "runtime:artifacts/feature/a.py", "repo:src/../bad.py"]:
        for windows in (True, False):
            result.append(dict(op="capability_within", value=dict(base, target_ref=target), ceiling=base, windows=windows))
    def member(identity, paths=None, services=None):
        return dict(branch_id=identity, resources=dict(writable_paths=paths or [], mutable_state_refs=[], exclusive_device_refs=[], services=services or []))
    members = [member("A", [dict(path="src", scope="subtree")]), member("B", [dict(path="src/a.py", scope="exact")]), member("C")]
    dependencies = [dict(before_branch_id="A", after_branch_id="C", reason="first"), dict(before_branch_id="C", after_branch_id="B", reason="then")]
    result += [dict(op="fanout_ordering", value=members, dependencies=[], case_sensitive=True), dict(op="fanout_ordering", value=members, dependencies=dependencies, case_sensitive=True)]
    service = dict(ref="gpu", units=1, capacity=2)
    members = [member(identity, services=[copy.deepcopy(service)]) for identity in ("A", "B", "C")]
    for edges in [[], [dict(before_branch_id="A", after_branch_id="B", reason="capacity")], [dict(before_branch_id="A", after_branch_id="B", reason="cycle"), dict(before_branch_id="B", after_branch_id="A", reason="cycle")]]:
        result.append(dict(op="fanout_ordering", value=members, dependencies=edges, case_sensitive=True))
    root = fixtures / "policy repo"
    for name in ("docs", "src", ".codex", "engineering-graph-artifacts"):
        (root / name).mkdir(parents=True)
    for name in ("AGENTS.md", "top.rs", "credentials.txt"):
        (root / name).write_text("fixture\n", encoding="utf-8")
    result.append(dict(op="policy_default_data", repo=str(root), roots=["docs/", "src/", "top.rs"]))
    return result


def oracle(request, hosts, execution, contracts, config, planner):
    function, host = request.get("function"), request.get("host", hosts.DEFAULT_HOST)
    try:
        if request["op"] == "glob":
            import fnmatch
            from pathlib import PurePosixPath
            result = PurePosixPath(request["value"]).match(request["pattern"]) if request["posix_path"] else fnmatch.fnmatchcase(request["value"], request["pattern"])
        elif request["op"] == "filesystem_capability_target":
            result = contracts.filesystem_capability_target(request["value"], request["denied"])
        elif request["op"] == "capability_within":
            import ntpath
            import posixpath
            from unittest.mock import patch
            with patch("graph_engine.contracts.os.path.normcase", ntpath.normcase if request["windows"] else posixpath.normcase):
                result = contracts.capability_within(request["value"], request["ceiling"])
        elif request["op"] == "fanout_ordering":
            result = planner.validate_fanout_ordering(request["value"], request["dependencies"], case_sensitive=request["case_sensitive"])
        elif request["op"] == "policy_default_data":
            result = config._default_profile_policy(Path(request["repo"]))
        elif request["op"] == "execution":
            if function == "build":
                result = execution.build_execution_plan("RUN-1", request["value"], request.get("size"), host)
            elif function == "reconstruct":
                result = execution.reconstruct_execution_plan("RUN-1", request["value"], request["stored"], request.get("size"))
            else:
                raise ValueError("UNKNOWN_EXECUTION_FUNCTION")
        elif function == "model_options":
            result = hosts.model_options(host, request.get("revision"))
        elif function == "recommended_assignment":
            result = hosts.recommended_assignment(host, request["workload"], request.get("revision"))
        elif function == "selected_dispatch_model":
            result = hosts.selected_dispatch_model(host, request["model"], request["effort"], request.get("revision"))
        elif function == "dispatch_weight_for":
            result = hosts.dispatch_weight_for(request["model"], request["effort"], historical=request["historical"])
        elif function == "supported_dispatch_weights":
            result = [[model, effort, weight] for (model, effort), weight in sorted(hosts.supported_dispatch_weights().items())]
        else:
            result = hosts.resolve_assignment(host, request["class"], request["effort"])
        return {"ok": True, "result": json.loads(json.dumps(result))}
    except contracts.ContractError as error:
        return {"ok": False, "error": {"kind": "ContractError", "field": error.field, "code": error.code}}
    except ValueError as error:
        return {"ok": False, "error": {"kind": "ValueError", "message": str(error)}}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--oracle", type=Path, required=True)
    parser.add_argument("--rust", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    sys.path.insert(0, str(args.oracle.resolve()))
    from graph_engine import hosts, execution, contracts, config, planner
    args.output.mkdir(parents=True, exist_ok=False)
    requests = cases(hosts, args.output / "fixtures")
    for request in requests:
        if request["op"] == "policy_default_data":
            request["repository_id"] = config._default_profile_policy(Path(request["repo"]))["repository_id"]
    encoded = b"".join((json.dumps(request, sort_keys=True)+"\n").encode("ascii") for request in requests)
    completed = subprocess.run([str(args.rust.resolve())], input=encoded, capture_output=True, timeout=120)
    (args.output / "requests.ndjson").write_bytes(encoded)
    (args.output / "stdout.ndjson").write_bytes(completed.stdout)
    (args.output / "stderr.bin").write_bytes(completed.stderr)
    actual = [json.loads(line) for line in completed.stdout.splitlines()]
    failures, expected_lines = [], []
    for index, request in enumerate(requests):
        expected = oracle(request, hosts, execution, contracts, config, planner)
        expected_lines.append(json.dumps(expected, sort_keys=True))
        if index >= len(actual) or not exact_equal(actual[index], expected):
            failures.append(dict(index=index, request=request, oracle=expected,
                                 native=actual[index] if index < len(actual) else None))
    (args.output / "oracle.ndjson").write_text("\n".join(expected_lines)+"\n", encoding="ascii")
    report = dict(scope="planning component only", requests=len(requests), responses=len(actual), native_exit=completed.returncode, failures=failures, binary_sha256=hashlib.sha256(args.rust.read_bytes()).hexdigest())
    report["bindings"] = capture(args.rust)
    (args.output / "report.json").write_text(json.dumps(report, indent=2)+"\n", encoding="ascii")
    first_failures = [dict(index=failure["index"], op=failure["request"]["op"], function=failure["request"].get("function"), value=failure["request"].get("value") if failure["request"]["op"]=="glob" else None, pattern=failure["request"].get("pattern"), oracle_error=failure["oracle"].get("error"), native_error=failure["native"].get("error") if failure["native"] else "MISSING_RESPONSE") for failure in failures[:8]]
    print(json.dumps(dict(requests=len(requests), responses=len(actual), failures=len(failures), native_exit=completed.returncode, first_failures=first_failures), ensure_ascii=True))
    return int(bool(failures) or completed.returncode != 0 or len(actual) != len(requests))


if __name__ == "__main__":
    raise SystemExit(main())
