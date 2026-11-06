"""Check helper policy normalization against the actual original fixture and validators."""

import argparse
import copy
import hashlib
import json
import subprocess
import sys
from pathlib import Path

from bindings import capture
from comparison import exact_equal


def materials(repository):
    command = dict(command_id="focused-tests", argv=[sys.executable, "-m", "unittest", "tests.test_contracts"], timeout_seconds=120)
    limits = dict(children=2, concurrency=1, commands=2, time_seconds=240, output_tokens=4000, file_reads=12)
    assignments = []
    for identifier, parent, role, commands in (("evidence", "tech_lead", "evidence_scout", []), ("validation", "senior_engineer", "validation_executor", [command])):
        capabilities = [dict(effect="filesystem_read", action="read", target_ref="repo:docs/")]
        capabilities += [dict(effect="command", action="run", target_ref=value["command_id"]) for value in commands]
        required = ["fresh_model_effort_selection", "filesystem_confinement", "tool_confinement"]
        if role == "validation_executor":
            required.append("command_confinement")
        assignments.append(dict(assignment_id=identifier, parent_role=parent, helper_role=role, contract_revision=1, model="gpt-6.1-sol", reasoning_effort="low", parent_capabilities=capabilities, scope_refs=["repo:docs/"], commands=commands, checkpoint_policy="observed_repository_state", resource_keys=["worktree"], required_host_capabilities=required, limits=copy.deepcopy(limits)))
    allowance = dict(schema_version=1, allowance_id="helpers-1", run_id="RUN-HELPERS", assignments=assignments, shared_limits=limits, resources=[dict(key="worktree", capacity=1)])
    observation = dict(schema_version=1, host_id="local-host", observed_at="2026-09-12T00:00:00Z", source="host capability probe", uncertainty="trusted caller evidence only", capabilities={name: dict(status="verified", source="host_api", uncertainty="none observed") for name in ("fresh_model_effort_selection", "filesystem_confinement", "tool_confinement", "command_confinement")}, supported_assignments=[dict(model="gpt-6.1-sol", reasoning_effort="low", status="verified", source="host_api", uncertainty="none observed")])
    test_mode = dict(disposable_repository=str(repository), acknowledge_unenforced_isolation=True, external_effects="mocked")
    return allowance, observation, test_mode


def changed(value, path, candidate):
    result = copy.deepcopy(value)
    current = result
    for key in path[:-1]:
        current = current[key]
    current[path[-1]] = candidate
    return result


def prepare(repository, helper):
    allowance, observation, test_mode = materials(repository)
    cases = []
    def add(operation, value, **arguments):
        cases.append(dict(op=operation, value=value, **arguments))
    defaults = dict(run_id="RUN-HELPERS", host="codex-astra", approved=None, historical=False)
    add("helper_allowance", allowance, **defaults)
    scalar = [None, True, False, -1, 0, 1, 1.0, "", "invalid", [], {}, [None]]
    paths = [(key,) for key in allowance]
    paths += [("shared_limits", key) for key in allowance["shared_limits"]]
    paths += [("resources", 0, key) for key in allowance["resources"][0]]
    paths += [("assignments", index, key) for index in (0, 1) for key in allowance["assignments"][index]]
    paths += [("assignments", index, "limits", key) for index in (0, 1) for key in allowance["shared_limits"]]
    paths += [("assignments", 0, "parent_capabilities", 0, key) for key in ("effect", "action", "target_ref")]
    for path in paths:
        for candidate in scalar:
            add("helper_allowance", changed(allowance, path, candidate), **defaults)
    for host in (None, "codex", "codex-astra", "cursor", "claude", "unknown", [], {}):
        for model, effort in (("gpt-5.6-luna", "max"), ("gpt-5.6-sol", "max"), ("gpt-6.1-sol", "low"), ("gpt-6.1-terra", "ultra"), ("gpt-6-astra", "medium"), ("opencode-go/muse-spark-1.3-contributor", "low"), ("unknown", "low"), ("gpt-5.6-\ud800", "low")):
            for historical in (False, True):
                record = copy.deepcopy(allowance)
                for assignment in record["assignments"]:
                    assignment.update(model=model, reasoning_effort=effort)
                add("helper_allowance", record, **{**defaults, "host": host, "historical": historical})
    approved = {item["parent_role"]: item["parent_capabilities"] for item in allowance["assignments"]}
    for candidate in (approved, {}, {"tech_lead": None}, {"tech_lead": [None]}, {"tech_lead": [dict(effect="filesystem_read", action=[], target_ref="repo:docs/")]}, [], True):
        add("helper_allowance", allowance, **{**defaults, "approved": candidate})
    test_allowance = {**allowance, "schema_version": 2, "test_mode": test_mode}
    add("helper_allowance", test_allowance, **defaults)
    for candidate in scalar:
        add("helper_test_mode", candidate)
        add("helper_limits", candidate)
        add("helper_commands", candidate)
        add("helper_parent_capabilities", candidate)
        add("helper_scope_refs", candidate)
        for allow in (False, True):
            add("helper_unique_ids", candidate, allow_empty=allow)
    for path in [(key,) for key in observation] + [("capabilities", name, key) for name in observation["capabilities"] for key in ("status", "source", "uncertainty")] + [("supported_assignments", 0, key) for key in observation["supported_assignments"][0]]:
        for candidate in scalar:
            add("helper_host_observation", changed(observation, path, candidate))
    add("helper_host_observation", observation)
    for key in test_mode:
        for candidate in scalar + ["relative/repo", str(repository)]:
            add("helper_test_mode", changed(test_mode, (key,), candidate))
    for candidate in scalar + [2**31 - 1, 2**31, 2**63 - 1, 2**63, -0.0, float("nan"), float("inf")]:
        for minimum, maximum in ((0, 2**31 - 1), (1, 1024), (0, 2**63 - 1)):
            add("helper_integer", candidate, minimum=minimum, maximum=maximum)
    canonical = lambda value: json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    plan = dict(schema_version=3, run_id="RUN-HELPERS", host="codex-astra", helper_allowance=dict(ref="repo:docs/helper-allowance.json", sha256="a" * 64))
    plan["plan_digest"] = hashlib.sha256(canonical(plan)).hexdigest()
    context = dict(schema_version=1, state_root_identity=dict(device=1, inode=2, path=str(repository)), repository_digest="a" * 64, run_id="RUN-HELPERS", plan_digest="b" * 64, allowance_ref="repo:docs/helper-allowance.json", allowance_digest="c" * 64, allowance_payload_digest="d" * 64, host_observation_digest="e" * 64, host_observation_payload_digest="f" * 64)
    context["register_id"] = hashlib.sha256(canonical({key: context[key] for key in ("state_root_identity", "repository_digest", "run_id")})).hexdigest()
    context["register_path"] = str(repository / "helper-registers" / context["register_id"] / "register.json")
    request = dict(schema_version=1, request_id="request-1", assignment_id="evidence", parent_role="tech_lead", helper_role="evidence_scout", contract_revision=1, model="gpt-6.1-sol", reasoning_effort="low", scope_refs=["repo:docs/"], commands=[], checkpoint_ref="repo:docs/checkpoint-request-1.json#sha256=" + "a" * 64, resource_keys=["worktree"], budgets=dict(time_seconds=60, output_tokens=1000, file_reads=3))
    for operation, value in (("helper_plan", plan), ("helper_context", context), ("helper_request", request), ("helper_filesystem_identity", context["state_root_identity"])):
        arguments = dict(run_id="RUN-HELPERS") if operation == "helper_plan" else {}
        add(operation, value, **arguments)
        for key in value:
            for candidate in scalar:
                add(operation, changed(value, (key,), candidate), **arguments)
    for key in request["budgets"]:
        for candidate in scalar:
            add("helper_request", changed(request, ("budgets", key), candidate))
    add("helper_amounts", request)
    normalized_allowance = helper.validate_allowance(allowance, "RUN-HELPERS", "codex-astra", historical=True)
    identity = dict(device=3, inode=4, path=str(repository))
    context = copy.deepcopy(context)
    context["repository_digest"] = hashlib.sha256(b"graph-repository-v1\0" + "\0".join(("3", "4", str(repository))).encode("utf-8")).hexdigest()
    context["allowance_payload_digest"] = hashlib.sha256(canonical(normalized_allowance)).hexdigest()
    context["host_observation_payload_digest"] = hashlib.sha256(canonical(helper.validate_host_observation(observation))).hexdigest()
    context["register_id"] = helper._register_key(context["state_root_identity"], context["repository_digest"], context["run_id"])
    context["register_path"] = str(repository / "helper-registers" / context["register_id"] / "register.json")
    empty = dict(schema_version=1, context=context, host="codex-astra", repository_identity=identity, allowance_ref=context["allowance_ref"], allowance=normalized_allowance, host_observation=helper.validate_host_observation(observation), usage={key: 0 for key in helper.LIMIT_KEYS}, active_concurrency=0, active_resources=dict(worktree=0), assignment_usage={row["assignment_id"]: {key: 0 for key in helper.LIMIT_KEYS} for row in normalized_allowance["assignments"]}, reservations={})
    active = copy.deepcopy(empty)
    amounts = helper._required_amounts(request)
    active["usage"] = {key: amount if key != "concurrency" else 0 for key, amount in amounts.items()}
    active["active_concurrency"] = 1
    active["active_resources"]["worktree"] = 1
    active["assignment_usage"]["evidence"] = amounts
    active["reservations"]["request-1"] = dict(request_digest=hashlib.sha256(canonical(request)).hexdigest(), assignment_id="evidence", status="active", reservation=amounts, resource_keys=["worktree"], settlement=None)
    for record in (empty, active):
        add("helper_record", record)
        add("helper_accounting", record, allowance=normalized_allowance)
        for key in record:
            for candidate in scalar:
                add("helper_record", changed(record, (key,), candidate))
    paths = [("usage", key) for key in helper.LIMIT_KEYS]
    paths += [("assignment_usage", "evidence", key) for key in helper.LIMIT_KEYS]
    paths += [("reservations", "request-1", key) for key in active["reservations"]["request-1"]]
    paths += [("reservations", "request-1", "reservation", key) for key in helper.LIMIT_KEYS]
    paths += [("context", key) for key in context]
    for path in paths:
        for candidate in scalar:
            add("helper_record", changed(active, path, candidate))
    return cases


def oracle(request, helper):
    try:
        value = request["value"]
        operation = request["op"]
        if operation == "helper_allowance":
            result = helper.validate_allowance(value, request["run_id"], request["host"], request["approved"], historical=request["historical"])
        elif operation == "helper_host_observation":
            result = helper.validate_host_observation(value)
        elif operation == "helper_plan":
            result = helper._validate_plan(value, request["run_id"])
        elif operation == "helper_context":
            result = helper._validate_context(value)
        elif operation == "helper_request":
            result = helper._validate_request(value)
        elif operation == "helper_amounts":
            result = helper._required_amounts(value)
        elif operation == "helper_filesystem_identity":
            result = helper._filesystem_identity(value, "f")
        elif operation == "helper_record":
            result = helper.RegisterRecordValidator.validate(value)
        elif operation == "helper_accounting":
            result = helper.RegisterAccountingInvariantChecker.check(value, request["allowance"])
        elif operation == "helper_test_mode":
            result = helper._validate_test_mode(value)
        elif operation == "helper_integer":
            result = helper._integer(value, "f", minimum=request["minimum"], maximum=request["maximum"])
        elif operation == "helper_unique_ids":
            result = helper._unique_ids(value, "f", allow_empty=request["allow_empty"])
        else:
            result = getattr(helper, "_" + operation.removeprefix("helper_"))(value, "f")
        return dict(ok=True, result=result)
    except Exception as error:
        detail = dict(kind=type(error).__name__)
        if hasattr(error, "field"):
            detail.update(field=error.field, code=error.code)
        elif hasattr(error, "code"):
            detail["code"] = error.code
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
    from graph_engine import helper_register
    args.output.mkdir(parents=True, exist_ok=False)
    repository = args.output.resolve() / "repository"
    repository.mkdir()
    requests = prepare(repository, helper_register)
    encoded = b"".join((json.dumps(request, sort_keys=True) + "\n").encode("ascii") for request in requests)
    completed = subprocess.run([str(args.rust.resolve())], input=encoded, capture_output=True, timeout=120)
    (args.output / "requests.ndjson").write_bytes(encoded)
    (args.output / "stdout.ndjson").write_bytes(completed.stdout)
    (args.output / "stderr.bin").write_bytes(completed.stderr)
    actual = [json.loads(line) for line in completed.stdout.splitlines()]
    expected, failures = [], []
    for index, request in enumerate(requests):
        response = oracle(request, helper_register)
        expected.append(response)
        observed = actual[index] if index < len(actual) else None
        if not exact_equal(response, observed):
            failures.append(dict(index=index, request=request, oracle=response, native=observed))
    (args.output / "oracle.ndjson").write_text("".join(json.dumps(response, sort_keys=True) + "\n" for response in expected), encoding="ascii")
    report = dict(scope="helper pure policy validators with originalfixture values; no reservations/CLI/originalidentity credit", requests=len(requests), responses=len(actual), native_exit=completed.returncode, failures=failures, bindings=capture(args.rust))
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="ascii")
    print(json.dumps(dict(requests=len(requests), responses=len(actual), native_exit=completed.returncode, failures=len(failures), first_failures=[dict(index=item["index"], oracle=item["oracle"], native=item["native"]) for item in failures[:8]])))
    return int(bool(failures) or completed.returncode != 0 or len(actual) != len(requests))


if __name__ == "__main__":
    raise SystemExit(main())
