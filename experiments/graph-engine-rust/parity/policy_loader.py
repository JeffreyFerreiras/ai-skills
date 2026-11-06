"""Exercise complete policy loading against the maintained Python implementation."""

import argparse
import copy
import json
import os
import subprocess
import sys
from pathlib import Path
from unittest.mock import patch

from bindings import capture
from comparison import exact_equal


def replace(value, path, candidate):
    result = copy.deepcopy(value)
    current = result
    for part in path[:-1]:
        current = current[part]
    current[path[-1]] = candidate
    return result


def records(base, legacy):
    yield "default-v3", base
    yield "original-v1-fixture", legacy
    older = copy.deepcopy(base)
    older["schema_version"] = 2
    older["artifact_roots"] = dict(repo=["generated/"], profile=["references/"])
    for capabilities in older["role_capabilities"].values():
        for capability in capabilities:
            if capability["target_ref"].startswith("runtime:"):
                capability["target_ref"] = "repo:generated/"
    yield "schema-v2", older
    old_routes = copy.deepcopy(base)
    del old_routes["routes"]["delivery_only"]
    yield "legacy-route-upgrade", old_routes
    scalar_cases = [None, True, False, 0, 1, 1.0, "", "invalid", [], {}, [[]], ["invalid"]]
    for key in base:
        removed = copy.deepcopy(base)
        del removed[key]
        yield "missing-"+key, removed
        for index, value in enumerate(scalar_cases):
            yield key+"-"+str(index), replace(base, [key], value)
    for version in (1, 2, 3, 4, True, 3.0, -1):
        yield "schema-"+repr(version), replace(base, ["schema_version"], version)
    for path in [
        ["compatible_engine","min"], ["compatible_engine","max_exclusive"],
        ["artifact_roots","repo"], ["artifact_roots","profile"], ["artifact_roots","runtime"],
        ["limits","design_revisions"], ["limits","delivery_repairs"], ["limits","inspection"],
        ["limits","inspection","file_reads"], ["limits","inspection","discovery_commands"],
        ["limits","manifest_bytes"], ["limits","artifact_bytes"],
        ["required_checks","diff-check"], ["required_checks","diff-check","command_id"],
        ["required_checks","diff-check","mandatory"], ["required_checks","diff-check","argv"],
        ["required_checks","diff-check","timeout_seconds"],
    ]:
        for index, value in enumerate(scalar_cases + [2, 3, 4, -1, 3601, 4194305]):
            yield ".".join(path)+"-"+str(index), replace(base, path, value)
    for node, template in base["node_templates"].items():
        for key in template:
            for index, value in enumerate(scalar_cases):
                yield "node-"+node+"-"+key+"-"+str(index), replace(base, ["node_templates",node,key], value)
    for role, capabilities in base["role_capabilities"].items():
        for index, value in enumerate(scalar_cases):
            yield "capabilities-"+role+"-"+str(index), replace(base, ["role_capabilities",role], value)
        for field in ("effect","action","target_ref"):
            for index, value in enumerate(scalar_cases):
                yield "capability-"+role+"-"+field+"-"+str(index), replace(base, ["role_capabilities",role,0,field], value)
        yield "duplicate-capability-"+role, replace(base, ["role_capabilities",role], capabilities + [copy.deepcopy(capabilities[0])])
    for kind in base["artifact_kinds"]:
        for field in ("extensions","max_bytes"):
            for index, value in enumerate(scalar_cases):
                yield "kind-"+kind+"-"+field+"-"+str(index), replace(base, ["artifact_kinds",kind,field], value)
    for value in [[], ["docs/","DOCS/"], ["src/file.rs"], ["src/","src/"], ["top.rs/"], ["src"], ["../outside/"], [".codex/"], ["src/*.rs"], [["src/"]]]:
        yield "implementation-path-"+repr(value), replace(base, ["implementation_roots"], value)
    for value in [[], ["references/"], ["references/","REFERENCES/"], ["elsewhere/"], ["references"], [["references/"]]]:
        yield "profile-root-"+repr(value), replace(base, ["artifact_roots","profile"], value)
    for value in [base["denied_patterns"] + [None], base["denied_patterns"] + [""], base["denied_patterns"] + [["invalid"]], base["denied_patterns"]*2]:
        yield "denials-"+repr(value[-1]), replace(base, ["denied_patterns"], value)
    for value in [None, {}, [], True]:
        yield "reviewer-config-"+repr(value), dict(copy.deepcopy(base), reviewer_delegation=value)


def prepare(config, output):
    fixture = output / "fixtures"
    repo, skill, default = (fixture / name for name in ("repo", "skill", "default-home"))
    for directory in (repo / "docs", repo / "src", repo / ".codex", skill):
        directory.mkdir(parents=True, exist_ok=True)
    for name in ("AGENTS.md","top.rs","credentials.txt"):
        (repo / name).write_text("fixture\n", encoding="utf-8")
    environment = dict(SOFTWARE_ENGINEERING_GRAPH_HOME=str(default))
    context = dict(repo=str(repo), skill=str(skill), environment=environment)
    requests = [dict(context, op="policy_default_native")]
    base = config._default_profile_policy(repo)
    legacy = json.loads((Path(config.SKILL_ROOT) / "tests/fixtures/engineering-graph.json").read_bytes())
    for index, (name, value) in enumerate(records(base, legacy)):
        home = fixture / "homes" / str(index)
        path = config.profile_policy_path(repo, home)
        path.parent.mkdir(parents=True)
        path.write_text(json.dumps(value, ensure_ascii=True), encoding="ascii")
        requests.append(dict(context, op="policy_load_native", runtime=str(home), fixture=name))
    for environment in [
        {}, {"SOFTWARE_ENGINEERING_GRAPH_HOME":""},
        {"SOFTWARE_ENGINEERING_GRAPH_HOME":"relative"},
        {"SOFTWARE_ENGINEERING_GRAPH_HOME":str(fixture / "primary"), "SOFTWARE_ENGINEERING_GRAPH_STATE_HOME":str(fixture / "secondary")},
        {"SOFTWARE_ENGINEERING_GRAPH_STATE_HOME":str(fixture / "secondary")},
        {"XDG_STATE_HOME":str(fixture / "xdg")},
    ]:
        for runtime in (None, "", "relative", str(fixture / "explicit")):
            requests.append(dict(context, environment=environment, op="state_root_native", runtime=runtime))
    return requests


def oracle(request, config, state):
    try:
        repo, skill = Path(request["repo"]), Path(request["skill"])
        explicit = request.get("runtime")
        runtime = None if explicit is None else Path(explicit)
        if request["op"] == "state_root_native":
            if explicit == "":
                runtime = ""
            result = str(state.resolve_state_root(runtime, environ=request["environment"], skill_root=skill))
        elif request["op"] == "policy_default_native":
            result = config._default_profile_policy(repo)
        else:
            with patch.dict(os.environ, request["environment"], clear=True):
                value, snapshot = config.load_policy(repo, skill_root=skill, runtime_root=runtime)
            result = dict(value=value, snapshot=dict(path=str(snapshot.path), digest=snapshot.digest,
                          size=snapshot.size, data=snapshot.data.hex(), identity=snapshot.identity,
                          parsed=snapshot.parsed))
        return dict(ok=True, result=json.loads(json.dumps(result)))
    except config.ContractError as error:
        return dict(ok=False, error=dict(kind="ContractError", field=error.field, code=error.code))
    except state.StateError as error:
        return dict(ok=False, error=dict(kind="StateError", code=error.code))
    except Exception as error:
        return dict(ok=False, error=dict(kind=type(error).__name__, message=str(error)))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--oracle", type=Path, required=True)
    parser.add_argument("--rust", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    sys.path.insert(0, str(args.oracle.resolve()))
    from graph_engine import config, state
    args.output.mkdir(parents=True, exist_ok=False)
    requests = prepare(config, args.output)
    encoded = b"".join((json.dumps(request, sort_keys=True)+"\n").encode("ascii") for request in requests)
    completed = subprocess.run([str(args.rust.resolve())], input=encoded, capture_output=True, timeout=180)
    (args.output / "requests.ndjson").write_bytes(encoded)
    (args.output / "stdout.ndjson").write_bytes(completed.stdout)
    (args.output / "stderr.bin").write_bytes(completed.stderr)
    actual = [json.loads(line) for line in completed.stdout.splitlines()]
    failures, expected = [], []
    for index, request in enumerate(requests):
        response = oracle(request, config, state)
        expected.append(response)
        if index >= len(actual) or not exact_equal(response, actual[index]):
            failures.append(dict(index=index, request=request, oracle=response, native=actual[index] if index < len(actual) else None))
    (args.output / "oracle.ndjson").write_text("".join(json.dumps(response,sort_keys=True)+"\n" for response in expected),encoding="ascii")
    report = dict(scope="complete loader schema/path checks on actual Windows ART fixtures; no Unix runtime claim", requests=len(requests),
                  responses=len(actual), native_exit=completed.returncode, failures=failures, bindings=capture(args.rust))
    (args.output / "report.json").write_text(json.dumps(report,indent=2)+"\n",encoding="ascii")
    first = [dict(index=item["index"], op=item["request"]["op"], fixture=item["request"].get("fixture"),
                  oracle_error=item["oracle"].get("error"), native_error=item["native"].get("error") if item["native"] else "MISSING_RESPONSE") for item in failures[:8]]
    print(json.dumps(dict(requests=len(requests),responses=len(actual),native_exit=completed.returncode,failures=len(failures),first_failures=first)))
    return int(bool(failures) or completed.returncode != 0 or len(actual) != len(requests))


if __name__ == "__main__":
    raise SystemExit(main())
