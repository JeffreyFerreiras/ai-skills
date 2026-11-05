"""Check artifact resolution, stored rows and fresh verification on actual ART files."""

import argparse
import copy
import dataclasses
import hashlib
import json
import sqlite3
import subprocess
import sys
import types
from contextlib import ExitStack
from pathlib import Path
from unittest.mock import patch

from bindings import capture
from comparison import exact_equal


def prepare(output, config, state, evidence, installation):
    repo, skill, runtime = (output / name for name in ("repo", "skill", "runtime"))
    for root in (repo, skill, runtime):
        root.mkdir()
    (repo / "docs").mkdir()
    (skill / "references").mkdir()
    policy = config._default_profile_policy(repo)
    runtime_relative = policy["artifact_roots"]["runtime"][0]
    (runtime / runtime_relative).mkdir(parents=True)
    content = b'{"answer":1}\n'
    digest = hashlib.sha256(content).hexdigest()
    file_refs = ["repo:docs/file.json", "profile:software-engineering-graph/references/file.json", "runtime:" + runtime_relative + "file.json"]
    for path in (repo / "docs/file.json", skill / "references/file.json", runtime / runtime_relative / "file.json"):
        path.write_bytes(content)
    (repo / "docs/empty.json").write_bytes(b"")
    (repo / "docs/wrong.txt").write_text("wrong", encoding="ascii")
    (repo / "docs/directory.json").mkdir()
    requests = []
    base = dict(policy=policy, repo=str(repo), skill=str(skill), installation=str(installation), runtime=str(runtime), environment={}, database=None, kind="collection")
    def add(operation, **values):
        record = copy.deepcopy(base)
        record.update(op=operation, **values)
        requests.append(record)
    for reference in file_refs:
        add("artifact_unhashed", reference=reference)
        add("artifact_resolve", reference=reference + "#sha256=" + digest, digest=digest)
    add("artifact_unhashed", reference=file_refs[0], fault_final_identity=True)
    for reference in (file_refs[0], "repo:docs/missing.json", "repo:docs/empty.json", "repo:docs/directory.json", "repo:other/file.json", "repo:docs/wrong.txt", "repo:docs/apostrophe's-é.json", 'repo:docs/double"quote.json', "repo:docs/missing-\ud800.json", "repo:docs/file.json/child.json"):
        for maximum in (-1, 0, 1, len(content), 2**1000, "bad", float("nan")):
            altered = copy.deepcopy(policy)
            altered["artifact_kinds"]["collection"]["max_bytes"] = maximum
            add("artifact_unhashed", reference=reference, policy=altered)
    for reference in (None, "invalid", "repo:docs/../file.json", "ledger:id", "profile:other/file.json", "repo:docs/file.json#sha256=" + "b" * 64):
        for separate in (None, "invalid", digest, "b" * 64):
            add("artifact_resolve", reference=reference, digest=separate)
    for extensions in (None, [], [None], [[".json"]], ".json", {".json": 1}, [".json"], [".JSON"]):
        altered = copy.deepcopy(policy)
        altered["artifact_kinds"]["collection"]["extensions"] = extensions
        add("artifact_unhashed", reference=file_refs[0], policy=altered)
    for version in (1, 2, 3, 3.0, None):
        altered = copy.deepcopy(policy)
        altered["schema_version"] = version
        altered["artifact_roots"]["repo"] = ["docs/"]
        for reference in file_refs:
            add("artifact_unhashed", reference=reference, policy=altered)
    canonical = evidence.canonical_ledger_artifact("ledger-1", "collection", {"b": 2, "a": "é"})
    bootstrap = dict(branch_id="bootstrap-branch", node_instance_id="bootstrap-instance", node_key="bootstrap", role="tech_lead", stage="bootstrap", generation=1, mandatory=True, status="ready", retry_count=0, max_retries=0, envelope_json="{}")
    task = dict(request_mode="delivery", minimum_route="full_delivery", inspection_budget=dict(file_reads=12, discovery_commands=8))
    databases = []
    for language in ("native", "oracle"):
        home = output / (language + "-state")
        store = state.StateStore(home)
        store.initialize(repo, dict(repository_id="fixture-repository", limits=dict(design_revisions=2, delivery_repairs=3)), "policy-digest", task, repo / "docs/file.json", "task-digest", "RUN-1", "init-1", "a" * 64, True, True, bootstrap, dict(size="small", plan_digest="f" * 64), "task-ref", [])
        databases.append(str(store.db_path("fixture-repository", "RUN-1")))
    base.update(database=databases[0], source_database=databases[1])
    record = dataclasses.asdict(canonical)
    add("artifact_persist", artifact=record)
    add("artifact_persist", artifact=record)
    add("artifact_resolve", reference=canonical.ref, digest=canonical.sha256)
    add("artifact_reverify", artifact=record)
    for key, value in (("content_json", None), ("content_json", "{"), ("content_json", "{}"), ("size_bytes", True), ("size_bytes", 99999999), ("sha256", "b" * 64), ("kind", "missing")):
        changed = copy.deepcopy(record)
        changed[key] = value
        add("artifact_reverify", artifact=changed)
    for key, value in (("content_json", "different"), ("size_bytes", 1), ("source_path", "other"), ("kind", "task_brief"), ("device", 0)):
        changed = copy.deepcopy(record)
        changed[key] = value
        add("artifact_persist", artifact=changed)
    add("artifact_reverify", artifact=record, content_hex=b"\xff".hex())
    add("artifact_reverify", artifact=record, content_hex=(canonical.content_json).encode("utf-16").hex())
    with sqlite3.connect(databases[1]) as connection:
        connection.row_factory = sqlite3.Row
        verified = evidence.resolve_unhashed_reference(file_refs[0], "collection", repo, skill, policy, runtime)
    file_record = dataclasses.asdict(verified)
    add("artifact_reverify", artifact=file_record)
    changed = copy.deepcopy(file_record)
    changed["inode"] = 0
    add("artifact_reverify", artifact=changed)
    return requests


def oracle(request, evidence):
    connection = None
    stack = ExitStack()
    try:
        if request.get("fault_final_identity"):
            import os
            original_fstat, original_lstat = os.fstat, Path.lstat
            calls = [0]
            def observed_fstat(descriptor):
                result = original_fstat(descriptor)
                calls[0] += 1
                return result
            def corrupted_lstat(path, *args, **kwargs):
                result = original_lstat(path, *args, **kwargs)
                if calls[0] >= 2:
                    fields = {name: getattr(result, name) for name in dir(result) if name.startswith("st_")}
                    fields["st_ino"] += 1
                    return types.SimpleNamespace(**fields)
                return result
            stack.enter_context(patch.object(evidence.os if hasattr(evidence, "os") else __import__("os"), "fstat", observed_fstat))
            stack.enter_context(patch.object(Path, "lstat", corrupted_lstat))
        if request.get("database"):
            connection = sqlite3.connect(request["source_database"], isolation_level=None)
            connection.row_factory = sqlite3.Row
            connection.execute("PRAGMA foreign_keys=ON")
            connection.execute("PRAGMA synchronous=FULL")
        repo, skill = Path(request["repo"]), Path(request["skill"])
        runtime = None if request["runtime"] is None else Path(request["runtime"])
        operation = request["op"]
        if operation == "artifact_resolve":
            result = dataclasses.asdict(evidence.resolve_reference(request["reference"], request["digest"], request["kind"], repo, skill, request["policy"], connection, runtime))
        elif operation == "artifact_unhashed":
            result = dataclasses.asdict(evidence.resolve_unhashed_reference(request["reference"], request["kind"], repo, skill, request["policy"], runtime))
        elif operation == "artifact_persist":
            result = evidence.persist_artifact(connection, "RUN-1", evidence.VerifiedArtifact(**request["artifact"]))
        else:
            record = copy.deepcopy(request["artifact"])
            if "content_hex" in request:
                record["content_json"] = bytes.fromhex(request["content_hex"])
            result = evidence.reverify_artifact(connection, record, repo, skill, request["policy"], runtime)
        return dict(ok=True, result=result)
    except Exception as error:
        detail = dict(kind=type(error).__name__)
        if hasattr(error, "field"):
            detail.update(field=error.field, code=error.code)
        else:
            detail["message"] = str(error)
        return dict(ok=False, error=detail)
    finally:
        stack.close()
        if connection is not None:
            connection.close()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--oracle", type=Path, required=True)
    parser.add_argument("--rust", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    sys.path.insert(0, str(args.oracle.resolve()))
    from graph_engine import config, state, evidence
    args.output.mkdir(parents=True, exist_ok=False)
    requests = prepare(args.output.resolve(), config, state, evidence, args.oracle.resolve())
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
    report = dict(scope="actual external files and independent stored artifact effects; no original identity credit", requests=len(requests), responses=len(actual), native_exit=completed.returncode, failures=failures, bindings=capture(args.rust))
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="ascii")
    print(json.dumps(dict(requests=len(requests), responses=len(actual), native_exit=completed.returncode, failures=len(failures), first_failures=[dict(index=item["index"], oracle=item["oracle"], native=item["native"]) for item in failures[:8]])))
    return int(bool(failures) or completed.returncode != 0 or len(actual) != len(requests))


if __name__ == "__main__":
    raise SystemExit(main())
