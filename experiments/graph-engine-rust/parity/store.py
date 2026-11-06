"""Exercise StateStore and URI private boundaries against the maintained oracle."""

import argparse
import copy
import json
import os
import re
import sqlite3
import subprocess
import sys
from pathlib import Path
from unittest.mock import patch

from bindings import capture
from comparison import exact_equal


class Injected(Exception):
    pass


class ProcessDeath(BaseException):
    pass


def prepare(output):
    fixture = output / "fixtures"
    repository, skill = (fixture / name for name in ("repo", "skill"))
    repository.mkdir(parents=True)
    skill.mkdir()
    (repository / "task.json").write_text("{}", encoding="utf-8")
    policy = dict(repository_id="fixture-repository", limits=dict(design_revisions=2, delivery_repairs=3))
    task = dict(request_mode="delivery", minimum_route="full_delivery", inspection_budget=dict(file_reads=12, discovery_commands=8))
    bootstrap = dict(branch_id="bootstrap-branch", node_instance_id="bootstrap-instance", node_key="bootstrap", role="tech_lead", stage="bootstrap", generation=1, mandatory=True, status="ready", retry_count=0, max_retries=0, envelope_json="{}")
    base = dict(op="state_store", environment={}, skill=str(skill), repo=str(repository), task_path=str(repository / "task.json"), policy=policy, task=task, bootstrap=bootstrap, execution_plan=dict(size="small", plan_digest="f" * 64), initial_artifacts=[], policy_digest="policy-digest", task_digest="task-digest", operation_id="init-1", request_digest="a" * 64, ack_permissions=True, ack_durability=True)
    requests = []
    def case(index, action, **overrides):
        value = copy.deepcopy(base)
        home = fixture / str(index)
        value.update(action=action, codex_home=str(home / "native"), source_home=str(home / "oracle"), state_root=None)
        value.update(overrides)
        requests.append(value)
    for index, candidate in enumerate((None, "", "relative", str(fixture / "explicit"))):
        case("root-" + str(index), "roots", codex_home=candidate)
    case("conflict", "roots", state_root=str(fixture / "state"))
    case("success", "initialize")
    case("success", "open")
    case("success", "initialize")
    case("success", "initialize", request_digest="b" * 64)
    case("success", "initialize", operation_id="other")
    for point in ("locking_probe", "database_file_flush", "directory_sync", "replace", "post_replace", "reopen", "identity_verification", "integrity_check"):
        case(point, "initialize", fault=point)
        case(point, "initialize")
    case("death", "initialize", fault="post_replace", process_death=True)
    case("death", "open")
    case("death", "initialize", request_digest="b" * 64)
    case("death", "initialize")
    case("ack", "initialize", ack_permissions=False, ack_durability=False)
    case("ack", "initialize", ack_durability=False)
    for action in ("host", "actor", "repository"):
        case("metadata", action)
    for process in (None, False, True, -1, 0, 1, os.getpid(), 2**31, 2**32 + 1, 2**100, -1.0, 0.0, 1.5, float("nan"), float("inf"), "1", [], {}):
        case("process", "process", process_id=process)
    import hashlib
    from graph_engine import state
    repository_digest = hashlib.sha256(b"graph-repository-v1\0fixture-repository").hexdigest()
    seeds = [(".state.upper.incomplete.sqlite3.MARKER", b"{")]
    for index, value in enumerate((None, [], "text", 1, {}, {"request_digest": "short"}, {"request_digest": "b" * 64}, {"request_digest": "a" * 64})):
        seeds.append((f".state.{index}.incomplete.sqlite3.marker", json.dumps(value).encode("utf-8")))
    seeds.append((".state.bad-utf8.incomplete.sqlite3.marker", b"\xff"))
    for value in (None, [], "text", {}, dict(host_identity="other", process_id=None), dict(host_identity=state.current_host_identity(), process_id=-1), dict(host_identity=state.current_host_identity(), process_id=os.getpid()), dict(host_identity=state.current_host_identity(), process_id=None), dict(host_identity=state.current_host_identity(), process_id=1.5)):
        seeds.append((".init.lock", json.dumps(value).encode("utf-8")))
    seeds.append((".init.lock", b"{"))
    for index, (name, content) in enumerate(seeds):
        identity = "seed-" + str(index)
        case(identity, "initialize")
        for language in ("native", "oracle"):
            root = fixture / identity / language / "graph-runs" / repository_digest / "RUN-1"
            root.mkdir(parents=True, mode=0o700)
            (root / name).write_bytes(content)
    for relative in ("task.json", "space #%.json", "éΣß.json", "missing/name.json", "missing/../name.json", "./task.json", "surrogate-\ud800.json", "null-\0.json"):
        requests.append(dict(op="sqlite_uri", path=str(repository / relative), environment={}))
    return requests


def source_request(request):
    value = copy.deepcopy(request)
    if request.get("source_home") and request.get("codex_home", "__absent__") not in (None, "", "relative"):
        value["codex_home"] = request["source_home"]
    return value


def oracle(request, state):
    request = source_request(request)
    try:
        if request["op"] == "sqlite_uri":
            return dict(ok=True, result=Path(request["path"]).resolve().as_uri() + "?mode=rw")
        action = request["action"]
        if action == "host":
            return dict(ok=True, result=state.current_host_identity())
        if action == "actor":
            return dict(ok=True, result=state.current_actor())
        if action == "process":
            return dict(ok=True, result=state._process_alive(request["process_id"]))
        def hook(point):
            if request.get("fault") == point:
                raise (ProcessDeath if request.get("process_death") else Injected)(point)
        environment = {key: value for key, value in os.environ.items() if key not in ("SOFTWARE_ENGINEERING_GRAPH_HOME", "SOFTWARE_ENGINEERING_GRAPH_STATE_HOME", "XDG_STATE_HOME")}
        environment.update(request["environment"])
        with patch.dict(os.environ, environment, clear=True), patch.object(state, "resolve_state_root", side_effect=lambda explicit=None, **kwargs: original_resolver(explicit, environ=environment, skill_root=Path(request["skill"]))):
            store = state.StateStore(None if request["codex_home"] is None else Path(request["codex_home"]), hook, state_root=None if request["state_root"] is None else Path(request["state_root"]))
            if action == "roots":
                result = dict(state_root=str(store.state_root), run_root=str(store.run_root("fixture-repository", "RUN-1")), inbox_root=str(store.inbox_root("fixture-repository", "RUN-1")), database=str(store.db_path("fixture-repository", "RUN-1")))
            elif action == "repository":
                result = list(state.repository_identity(Path(request["repo"])))
            elif action == "open":
                with store.open_run("fixture-repository", "RUN-1") as connection:
                    result = state.StateStore.row_dict(connection.execute("SELECT * FROM runs LIMIT 1").fetchone())
            else:
                result = store.initialize(Path(request["repo"]), request["policy"], request["policy_digest"], request["task"], Path(request["task_path"]), request["task_digest"], "RUN-1", request["operation_id"], request["request_digest"], request["ack_permissions"], request["ack_durability"], request["bootstrap"], request["execution_plan"], "task-ref", request["initial_artifacts"])
        return dict(ok=True, result=result)
    except BaseException as error:
        detail = dict(kind=type(error).__name__)
        if hasattr(error, "field"):
            detail["field"] = error.field
        if hasattr(error, "code"):
            detail["code"] = error.code
        else:
            detail["message"] = str(error)
        return dict(ok=False, error=detail)


def normalized(response, request, native):
    result = copy.deepcopy(response)
    if not result.get("ok") or request["op"] == "sqlite_uri":
        return result
    value = result["result"]
    if not isinstance(value, dict):
        return result
    home = request.get("codex_home") if native else source_request(request).get("codex_home")
    if home:
        home = str(Path(home))
        for key in ("state_root", "run_root", "inbox_root", "database", "inbox"):
            if key in value and value[key].startswith(home):
                value[key] = "<state-root>" + value[key][len(home):]
    if request.get("action") == "open":
        import hashlib
        digest = hashlib.sha256(b"graph-repository-v1\0fixture-repository").hexdigest()
        path = Path(home) / "graph-runs" / digest / "RUN-1/state.sqlite3"
        info = path.stat()
        assert (value["database_device"], value["database_inode"]) == (info.st_dev, info.st_ino)
        assert re.fullmatch(r"\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ", value["started_at"])
        value.update(database_device="<actual-database-device>", database_inode="<actual-database-inode>", started_at="<whole-second-UTC>")
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--oracle", type=Path, required=True)
    parser.add_argument("--rust", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    sys.path.insert(0, str(args.oracle.resolve()))
    from graph_engine import state
    global original_resolver
    original_resolver = state.resolve_state_root
    args.output.mkdir(parents=True, exist_ok=False)
    requests = prepare(args.output.resolve())
    encoded = b"".join((json.dumps(request, sort_keys=True) + "\n").encode("ascii") for request in requests)
    completed = subprocess.run([str(args.rust.resolve())], input=encoded, capture_output=True, timeout=120)
    (args.output / "requests.ndjson").write_bytes(encoded)
    (args.output / "stdout.ndjson").write_bytes(completed.stdout)
    (args.output / "stderr.bin").write_bytes(completed.stderr)
    actual = [json.loads(line) for line in completed.stdout.splitlines()]
    expected, failures = [], []
    for index, request in enumerate(requests):
        response = oracle(request, state)
        expected.append(response)
        observed = actual[index] if index < len(actual) else None
        if observed is None or not exact_equal(normalized(response, request, False), normalized(observed, request, True)):
            failures.append(dict(index=index, request=request, oracle=response, native=observed))
    (args.output / "oracle.ndjson").write_text("".join(json.dumps(response, sort_keys=True) + "\n" for response in expected), encoding="ascii")
    report = dict(scope="StateStore/URI private external scenarios; independent per-language homes; actual DB identity and UTC format assertions before explicit nondeterministic labels; no original identity credit", requests=len(requests), responses=len(actual), native_exit=completed.returncode, failures=failures, bindings=capture(args.rust))
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="ascii")
    print(json.dumps(dict(requests=len(requests), responses=len(actual), native_exit=completed.returncode, failures=len(failures), first_failures=[dict(index=item["index"], oracle=item["oracle"], native=item["native"]) for item in failures[:8]])))
    return int(bool(failures) or completed.returncode != 0 or len(actual) != len(requests))


if __name__ == "__main__":
    raise SystemExit(main())
