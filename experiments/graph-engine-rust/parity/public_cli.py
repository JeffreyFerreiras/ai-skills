"""External CLI comparison on a shared repository and independent state roots."""

import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import sys
import tempfile

from comparison import exact_equal
from workload_io import passed, raw, read_json, require_exit, run_child, sha256, write_json


GIT_OVERRIDES = (
    "GIT_DIR", "GIT_WORK_TREE", "GIT_COMMON_DIR", "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES",
)
ROOT_OVERRIDES = ("SOFTWARE_ENGINEERING_GRAPH_HOME", "SOFTWARE_ENGINEERING_GRAPH_STATE_HOME", "XDG_STATE_HOME")


def binary(directory, name):
    return directory / (name + (".exe" if os.name == "nt" else ""))


def cli_commands(oracle, native, leaf):
    return (
        [sys.executable, "-I", "-S", "-B", oracle / "scripts" / f"{leaf}.py"],
        [binary(native, leaf)],
    )


def pair(output, label, commands, cwd, env):
    return tuple(
        run_child(output, f"{label}-{side}", argv, cwd, env, timeout=120)
        for side, argv in zip(("oracle", "native"), commands, strict=True)
    )


def byte_comparison(output, label, commands, cwd, env, exit_code):
    oracle, native = pair(output, label, commands, cwd, env)
    return {
        "label": label, "processes": [oracle, native], "expected_exit": exit_code,
        "matches": passed(oracle, exit_code) and passed(native, exit_code)
        and all(raw(oracle, stream) == raw(native, stream) for stream in ("stdout", "stderr")),
    }


def help_and_parser(output, oracle, native, coverage, env):
    results = []
    for index, key in enumerate(["graphctl", "helper-register", *sorted(coverage["commands"])]):
        leaf, *arguments = shlex.split(key)
        commands = [base + [*arguments, "--help"] for base in cli_commands(oracle, native, leaf)]
        row = byte_comparison(output, f"help-{index:02}", commands, oracle, env, 0)
        row["command"] = key
        results.append(row)
    for leaf in ("graphctl", "helper-register"):
        commands = [base + ["--unknown-parity-option"] for base in cli_commands(oracle, native, leaf)]
        row = byte_comparison(output, f"parser-negative-{leaf}", commands, oracle, env, 2)
        row["command"] = leaf
        results.append(row)
    return results


def task(policy_sha):
    return {
        "schema_version": 1, "task_id": "TASK-1", "objective": "Exercise the graph",
        "user_outcome": "Deterministic evidence", "request_mode": "delivery",
        "minimum_route": "full_delivery", "mandatory_impact_tags": [],
        "scope": {"included": ["graph"], "excluded": ["application"]},
        "constraints": ["preserve runtime"], "risk_level": "low",
        "acceptance_criteria": [{"id": "AC-001", "text": "Graph closes"}],
        "authority": {"capabilities": [{"effect": "filesystem_read", "action": "read", "target_ref": "repo:docs/"}]},
        "policy_approval": {"sha256": policy_sha, "authority_ref": "authority:test"},
        "evidence_paths": ["repo:docs/engineering-graph.md"],
        "inspection_budget": {"file_reads": 12, "discovery_commands": 8},
        "required_check_ids": ["repo-check"], "required_human_decisions": [],
    }


def check_fixture(root, oracle):
    fixture = Path(tempfile.mkdtemp(prefix="pc", dir=root / "fixtures")).resolve(strict=True)
    if fixture.parent != (root / "fixtures").resolve(strict=True):
        raise ValueError("public parity fixture escaped its owned parent")
    repo = fixture / "repo"
    docs = repo / "docs"
    docs.mkdir(parents=True)
    (docs / "engineering-graph.md").write_text("# Test graph evidence\n", encoding="utf-8")
    source = docs / "source.txt"
    source.write_bytes(b"A")
    policy_path = oracle / "tests/fixtures/engineering-graph.json"
    policy = read_json(policy_path)
    policy["required_checks"]["repo-check"]["argv"] = [sys.executable, "-c", "import sys; sys.exit(0)"]
    policy_bytes = json.dumps(policy, ensure_ascii=True).encode("ascii")
    policy_sha = hashlib.sha256(policy_bytes).hexdigest()
    identity = str(repo.resolve(strict=True))
    if os.name == "nt":
        identity = identity.casefold()
    filename = "repo-" + hashlib.sha256(identity.encode("utf-8")).hexdigest()[:24] + ".json"
    state_roots = (fixture / "oracle", fixture / "native")
    for state in state_roots:
        policies = state / "policies"
        policies.mkdir(parents=True, mode=0o700)
        policy_file = policies / filename
        policy_file.write_bytes(policy_bytes)
        if os.name != "nt":
            state.chmod(0o700)
            policy_file.chmod(0o600)
    task_path = docs / "task.json"
    write_json(task_path, task(policy_sha))
    return {
        "fixture": str(fixture), "repo": repo, "state_roots": state_roots,
        "source": source, "task_path": task_path, "task_sha256": sha256(task_path),
        "policy_sha256": policy_sha, "stock_policy_sha256": sha256(policy_path),
    }


def constraints(output, root, fixture, cli, env):
    frozen = root / "constraints-reference"
    relative = ("context.json", "bundle.json", "bad-bundle.json")
    checks = fixture["repo"] / "docs/constraints"
    checks.mkdir()
    bindings = {}
    for filename in relative:
        data = (frozen / filename).read_bytes()
        path = checks / filename
        path.write_bytes(data)
        bindings[filename] = hashlib.sha256(data).hexdigest()
    results = []
    for name, code in (("bundle.json", 0), ("bad-bundle.json", 4)):
        arguments = ["constraints", "select", "--bundle", checks / name, "--context", checks / "context.json"]
        results.append(byte_comparison(
            output, f"constraints-{code}", [base + arguments for base in cli], fixture["repo"], env, code,
        ))
    return {"inputs": bindings, "comparisons": results}


def stable_status(value):
    return {
        "fields": {key: value[key] for key in ("ok", "code", "run_id", "state_revision", "status", "route", "state_schema_version")},
        "branches": [{key: item[key] for key in ("branch_id", "node_key", "role", "status")} for item in value["branches"]],
        "checks": [{key: item[key] for key in ("check_id", "outcome")} for item in value["check_evidence"]],
        "next_action": value["next_action"],
    }


def check_summary(value):
    digest = value["artifact_sha256"]
    if not re.fullmatch(r"[0-9a-f]{64}", digest) or not value["artifact_ref"].endswith(f"#sha256={digest}"):
        raise ValueError("check receipt reference and digest disagree")
    return {key: value[key] for key in ("ok", "code", "run_id", "state_revision", "check_id", "outcome")}


def check_trace(output, fixture, cli, env):
    repo = fixture["repo"]
    git = []
    rows = []

    def git_step(label, *arguments):
        record = run_child(output, f"git-{label}", ["git", *arguments], repo, env, timeout=60)
        git.append(record)
        require_exit(record)
        return raw(record, "stdout")

    def step(label, arguments, expected_code, project=lambda value: value, exit_code=0):
        records = pair(output, label, [base + arguments for base in cli], repo, env)
        row = {
            "label": label, "expected_code": expected_code, "expected_exit": exit_code,
            "processes": records, "matches": False,
        }
        rows.append(row)
        if not all(passed(record, exit_code) for record in records):
            return None
        values = [json.loads(raw(record, "stdout")) for record in records]
        if any(value.get("code") != expected_code for value in values):
            row["observed_codes"] = [value.get("code") for value in values]
            return None
        projected = [project(value) for value in values]
        row.update(
            projections=projected,
            matches=exact_equal(*projected) and all(not raw(record, "stderr") for record in records),
        )
        return values if row["matches"] else None

    identity = ["--run-id", "RUN-1"]
    commit = ["-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit"]
    result = {
        "fixture": fixture["fixture"], "task_sha256": fixture["task_sha256"],
        "policy_sha256": fixture["policy_sha256"], "stock_policy_sha256": fixture["stock_policy_sha256"],
        "git": git, "comparisons": rows, "passed": False,
        "projection": "stable run/branch/check fields and next action; timestamped receipt identities retained in raw streams",
    }
    try:
        git_step("init", "init")
        git_step("add", "add", "docs/source.txt")
        git_step("commit", *commit, "-m", "fixture")
        old_head = git_step("old-head", "rev-parse", "HEAD")
        old_tree = git_step("old-tree", "rev-parse", "HEAD^{tree}")
        initialized = step(
            "init",
            ["--ack-degraded-permissions", "--ack-degraded-durability", "init", *identity,
             "--task-brief", fixture["task_path"], "--op-id", "init-1"],
            "INITIALIZED",
            lambda value: {key: value[key] for key in ("ok", "code", "run_id", "state_revision", "execution_plan_digest")},
        )
        if initialized is None:
            return result
        approved = step(
            "approve",
            ["record", "plan-approval", *identity, "--plan-digest", initialized[0]["execution_plan_digest"],
             "--decision", "APPROVE", "--authority-ref", "authority:test", "--op-id", "plan-approval-1"],
            "EXECUTION_PLAN_APPROVED",
        )
        if approved is None:
            return result
        checked = step(
            "check", ["check", "run", *identity, "--check-id", "repo-check", "--op-id", "legacy-check"],
            "CHECK_RECORDED", check_summary,
        )
        if checked is None:
            return result
        fresh = step("fresh-status", ["status", *identity], "STATUS", stable_status)
        fixture["source"].write_bytes(b"B")
        stale = step("changed-status", ["status", *identity], "CHECK_EVIDENCE_PROVENANCE_INVALID", exit_code=4)
        fixture["source"].write_bytes(b"A")
        restored = step("restored-status", ["status", *identity], "STATUS", stable_status)
        git_step("empty-commit", *commit, "--allow-empty", "-m", "same tree")
        new_head = git_step("new-head", "rev-parse", "HEAD")
        new_tree = git_step("new-tree", "rev-parse", "HEAD^{tree}")
        result["changed_head_same_tree"] = old_head != new_head and old_tree == new_tree
        changed_head = step("changed-head-status", ["status", *identity], "CHECK_EVIDENCE_PROVENANCE_INVALID", exit_code=4)
        result["passed"] = (
            result["changed_head_same_tree"]
            and all(item is not None for item in (fresh, stale, restored, changed_head))
            and all(value["outcome"] == "PASS" for value in checked)
        )
    except (OSError, ValueError, KeyError) as error:
        result["error"] = str(error)
    return result


def run_public(output, root, oracle, native, coverage, env):
    output.mkdir()
    env = {key: value for key, value in env.items() if key not in GIT_OVERRIDES + ROOT_OVERRIDES}
    help_rows = help_and_parser(output, oracle, native, coverage, env)
    fixture = check_fixture(root, oracle)
    cli = [
        base + ["--repo", fixture["repo"], "--state-root", state]
        for base, state in zip(cli_commands(oracle, native, "graphctl"), fixture["state_roots"], strict=True)
    ]
    constraints_report = constraints(output, root, fixture, cli, env)
    trace = check_trace(output, fixture, cli, env)
    result = {
        "help_and_parser": help_rows, "constraints": constraints_report, "freshness": trace,
        "removed_fixture_environment_keys": list(GIT_OVERRIDES + ROOT_OVERRIDES),
        "passed": trace["passed"] and all(row["matches"] for row in help_rows + constraints_report["comparisons"]),
    }
    write_json(output / "report.json", result)
    return result
