"""Bounded original usage workflows against owned copies of one native schema seed."""

import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import secrets
import shutil
import sqlite3
import sys
from types import SimpleNamespace
from unittest.mock import patch

from probe_usage_pure import counters, line, sample


def scenarios():
    cases = []

    def case(name):
        steps = []
        cases.append({"name": name, "initial_hex": sample([]).hex(), "steps": steps})
        return steps

    def tokens(steps, current, last=None):
        steps.append({"kind": "append", "data_hex": line("event_msg", {"type": "token_count", "info": {
            "total_token_usage": current, "last_token_usage": current if last is None else last}}).hex()})

    def call(steps, action, operation, **fields):
        steps.append({"kind": "call", "action": action, "operation": operation,
                      "label": operation, **fields})

    steps = case("empty_report")
    steps.append({"kind": "report"})
    steps = case("historical_checkpoint")
    tokens(steps, counters(100, 20))
    steps.append({"kind": "checkpoint", "label": "before"})
    tokens(steps, counters(160, 30), counters(60, 10))
    call(steps, "bind", "bind", checkpoint_from="before")
    call(steps, "close", "close", binding_from="bind")
    steps = case("baseline_replay")
    tokens(steps, counters(100, 20))
    call(steps, "bind", "bind")
    tokens(steps, counters(120, 25), counters(20, 5))
    call(steps, "collect", "collect", binding_from="bind")
    tokens(steps, counters(120, 25), counters(20, 5))
    call(steps, "collect", "duplicate", binding_from="bind")
    tokens(steps, counters(140, 30), counters(20, 5))
    call(steps, "collect", "collect", binding_from="bind")
    call(steps, "close", "collect", binding_from="bind")
    call(steps, "close", "close", binding_from="bind")
    steps = case("phase_bridge_overlap")
    tokens(steps, counters())
    call(steps, "bind", "bind")
    call(steps, "bind", "overlap", phase="implementation")
    tokens(steps, counters(20, 2), counters(10, 1))
    call(steps, "close", "close", binding_from="bind")
    call(steps, "bind", "next", phase="implementation")
    tokens(steps, counters(40, 4), counters(30, 3))
    call(steps, "close", "next-close", binding_from="next")
    steps = case("prerun_bridge_reset")
    tokens(steps, counters())
    call(steps, "bind", "bind")
    tokens(steps, counters(30, 3), counters(30, 3))
    call(steps, "collect", "gap", binding_from="bind")
    tokens(steps, counters(40, 4), counters(10, 1))
    tokens(steps, counters(5, 1))
    tokens(steps, counters(15, 2), counters(10, 1))
    call(steps, "close", "close", binding_from="bind")
    steps = case("optional_alias_discontinuity")
    tokens(steps, {**counters(100, 20), "cached_input_tokens": 30, "reasoning_output_tokens": 10,
                   "cache_write_input_tokens": 12, "cache_write_tokens": 99})
    call(steps, "bind", "bind")
    tokens(steps, {**counters(120, 25), "cached_input_tokens": 10, "reasoning_output_tokens": 26,
                   "cache_write_input_tokens": 15}, counters(20, 5))
    call(steps, "collect", "collect", binding_from="bind")
    tokens(steps, {**counters(140, 30), "cache_write_input_tokens": "invalid", "cache_write_tokens": 200}, counters(20, 5))
    call(steps, "close", "close", binding_from="bind")
    steps = case("all_phases_contexts")
    tokens(steps, counters())
    for index, phase in enumerate(("scoping", "research_design", "implementation", "review_testing", "closure")):
        effort = "medium" if index % 2 == 0 else "high"
        steps.append({"kind": "append", "data_hex": line("turn_context", {"model": "gpt-6-astra", "effort": effort}).hex()})
        call(steps, "bind", "bind-" + phase, phase=phase)
        tokens(steps, counters(20 + index * 10, 2 + index), counters(10, 1))
        call(steps, "close", "close-" + phase, binding_from="bind-" + phase)
    steps = case("missing_resume")
    tokens(steps, counters())
    call(steps, "bind", "bind")
    tokens(steps, counters(20, 2), counters(10, 1))
    call(steps, "close", "close", binding_from="bind")
    steps.append({"kind": "unlink"})
    call(steps, "bind", "missing")
    call(steps, "close", "missing-close", binding_from="missing")
    steps = case("rewritten_truncated_prefix")
    steps.insert(0, {"kind": "append", "data_hex": line("session_meta", {"id": "synthetic-session", "cwd": "PRIVATE_SOURCE_PATH"}).hex()})
    tokens(steps, counters())
    call(steps, "bind", "bind")
    tokens(steps, counters(20, 2), counters(10, 1))
    call(steps, "collect", "collect", binding_from="bind")
    steps.append({"kind": "replace", "old": "PRIVATE_SOURCE_PATH", "new": "ALTERED_SOURCEPATHX"})
    call(steps, "collect", "rewrite", binding_from="bind")
    steps.append({"kind": "truncate", "size": 60})
    call(steps, "close", "close", binding_from="bind")
    steps = case("late_aborted_large_aggregate")
    tokens(steps, counters(0, 0))
    call(steps, "bind", "bind")
    tokens(steps, counters(10, 0))
    call(steps, "collect", "collect", binding_from="bind")
    tokens(steps, counters(0, 0))
    tokens(steps, counters((1 << 63) - 1, 0))
    steps.append({"kind": "abort_row"})
    call(steps, "close", "close", binding_from="bind")
    steps = case("corrupt_optional_state_and_later_bad_text")
    tokens(steps, counters())
    call(steps, "bind", "bind")
    steps.append({"kind": "corrupt_json"})
    steps.append({"kind": "append_bad_text"})
    steps.append({"kind": "report"})
    call(steps, "collect", "invalid-state", binding_from="bind")
    steps.append({"kind": "restore_json"})
    steps.append({"kind": "report"})
    steps = case("prepared_checkpoint_compare_and_swap")
    tokens(steps, counters())
    call(steps, "bind", "bind")
    tokens(steps, counters(20, 2), counters(10, 1))
    call(steps, "collect", "loser", binding_from="bind", interleave="winner")
    steps.append({"kind": "report"})
    return cases


def bytes_value(value):
    if isinstance(value, bytes):
        return {"sqlite_bytes_hex": value.hex()}
    return value


def snapshot(connection):
    run = dict(connection.execute("SELECT state_revision,status FROM runs WHERE run_id='RUN-1'").fetchone())
    operations = [dict(row) for row in connection.execute(
        "SELECT operation_id,request_digest,CAST(response_json AS BLOB) AS response_bytes,typeof(response_json) AS response_type,resulting_revision FROM operations WHERE run_id='RUN-1' ORDER BY operation_id")]
    events = [dict(row) for row in connection.execute(
        "SELECT event_id,run_id,revision,event_type,source_id,CAST(detail_json AS BLOB) AS detail_bytes,typeof(detail_json) AS detail_type FROM events WHERE run_id='RUN-1' ORDER BY event_id")]
    for row in operations + events:
        for key, value in row.items():
            row[key] = bytes_value(value)
    return {"run": run, "operations": operations, "events": events}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--seed", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    artifact = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve()
    seed = args.seed.resolve()
    seed.relative_to(artifact)
    if seed.name != "seed.sqlite3":
        raise RuntimeError("only the named owned seed is permitted")
    seed_hash = hashlib.sha256(seed.read_bytes()).hexdigest()
    output = args.output.resolve()
    output.relative_to(artifact)
    output.mkdir(parents=True, exist_ok=False)
    fixtures = (artifact / "fixtures").resolve()
    repo = Path(__file__).resolve().parents[3]
    sys.path.insert(0, str(repo / "skills/software-engineering-graph"))
    from graph_engine import usage
    from graph_engine.state import StateStore
    active = False

    class ScopeViolation(BaseException):
        pass

    def audit(event, arguments):
        if not active:
            return
        if event in {"open", "sqlite3.connect"} and arguments and not isinstance(arguments[0], int):
            candidate = Path(os.fsdecode(arguments[0])).resolve()
            if not candidate.is_relative_to(artifact):
                raise ScopeViolation("unexpected original IO outside ART: " + str(candidate))
        if event.startswith("subprocess.") or event == "socket.connect":
            raise ScopeViolation("unexpected child/network effect: " + event)

    sys.addaudithook(audit)

    @contextmanager
    def scope():
        nonlocal active
        previous = active
        active = True
        try:
            yield
        finally:
            active = previous

    counts = {"checkpoint": 0, "record": 0, "report": 0, "mutate": 0}

    def count(kind):
        if sum(counts.values()) >= 200:
            raise ScopeViolation("authorized API-call bound exceeded")
        counts[kind] += 1

    original_report, original_mutate = usage.report, StateStore.mutate

    def report(*arguments, **keywords):
        count("report")
        return original_report(*arguments, **keywords)

    def mutate(self, *arguments, **keywords):
        count("mutate")
        return original_mutate(self, *arguments, **keywords)

    def outcome(function):
        try:
            with scope():
                value = function()
            return {"ok": True, "value": value, "stored_hex": json.dumps(value, ensure_ascii=True).encode("ascii").hex()}
        except Exception as error:
            return {"ok": False, "type": type(error).__name__, "message": str(error)}

    cases = scenarios()
    if len(cases) != 12:
        raise RuntimeError("unexpected scenario count")
    (output / "actions.json").write_text(json.dumps(cases, ensure_ascii=True) + "\n", encoding="ascii")
    results = []
    with patch.object(usage, "report", report), patch.object(StateStore, "mutate", mutate):
        for case in cases:
            root = fixtures / ("usage-db-source-" + secrets.token_hex(12))
            root.mkdir()
            root.resolve().relative_to(fixtures)
            database = root / "usage.sqlite3"
            shutil.copyfile(seed, database)
            log = root / "synthetic-session.jsonl"
            log.write_bytes(bytes.fromhex(case["initial_hex"]))
            connection = sqlite3.connect(str(database))
            connection.row_factory = sqlite3.Row
            connection.execute("PRAGMA foreign_keys=ON")
            with scope():
                store = StateStore(state_root=root / "state")
            labels, rows = {}, []
            original_json = None

            def call(step):
                count("record")
                checkpoint = labels.get(step.get("checkpoint_from"), {})
                args_value = SimpleNamespace(
                    op_id=step["operation"], action=step["action"], session_log=str(log),
                    start_offset=checkpoint.get("offset"), source_id=checkpoint.get("source_id"),
                    prefix_sha256=checkpoint.get("prefix_sha256"),
                    binding_id=labels[step["binding_from"]]["binding_id"] if "binding_from" in step else None,
                    branch_id=None, attempt_id=None,
                    phase=step.get("phase", "scoping") if step["action"] == "bind" else None,
                    generation=0 if step["action"] == "bind" else None)
                run = connection.execute("SELECT * FROM runs WHERE run_id='RUN-1'").fetchone()
                return usage.record(args_value, connection, run, store, lambda *arguments: None)

            for step in case["steps"]:
                kind = step["kind"]
                value = None
                if kind == "append":
                    with log.open("ab") as handle:
                        handle.write(bytes.fromhex(step["data_hex"]))
                elif kind == "unlink":
                    log.unlink()
                elif kind == "replace":
                    log.write_bytes(log.read_bytes().replace(step["old"].encode("ascii"), step["new"].encode("ascii")))
                elif kind == "truncate":
                    with log.open("r+b") as handle:
                        handle.truncate(step["size"])
                elif kind == "checkpoint":
                    count("checkpoint")
                    value = outcome(lambda: usage.checkpoint(str(log)))
                elif kind == "report":
                    value = outcome(lambda: usage.report(connection, connection.execute("SELECT * FROM runs WHERE run_id='RUN-1'").fetchone()))
                elif kind == "abort_row":
                    connection.execute("UPDATE runs SET status='aborted' WHERE run_id='RUN-1'")
                    connection.commit()
                elif kind == "corrupt_json":
                    original_json = connection.execute("SELECT detail_json FROM events WHERE event_type='usage.observation' ORDER BY event_id").fetchone()[0]
                    connection.execute("UPDATE events SET detail_json='{bad}' WHERE event_type='usage.observation'")
                    connection.commit()
                elif kind == "append_bad_text":
                    connection.execute("INSERT INTO events(run_id,revision,event_type,source_id,detail_json) SELECT run_id,revision,event_type,source_id,CAST(X'ff' AS TEXT) FROM events WHERE event_type='usage.observation' LIMIT 1")
                    connection.commit()
                elif kind == "restore_json":
                    connection.execute("UPDATE events SET detail_json=? WHERE event_id=(SELECT MIN(event_id) FROM events WHERE event_type='usage.observation')", (original_json,))
                    connection.commit()
                elif kind == "call":
                    if "interleave" in step:
                        bound_mutate = store.mutate
                        interleaved = []

                        def interleave(*arguments, **keywords):
                            with patch.object(store, "mutate", bound_mutate):
                                interleaved.append(outcome(lambda: call({**step, "operation": step["interleave"]})))
                            return bound_mutate(*arguments, **keywords)

                        with patch.object(store, "mutate", side_effect=interleave):
                            value = outcome(lambda: call(step))
                        value["interleaved"] = interleaved
                    else:
                        value = outcome(lambda: call(step))
                else:
                    raise ScopeViolation("unregistered scenario step: " + kind)
                if value and value.get("ok") and "label" in step:
                    labels[step["label"]] = value["value"]
                rows.append({"step": step, "outcome": value, "snapshot": snapshot(connection)})
            time_columns = [row["name"] for row in connection.execute("PRAGMA table_info(events)")
                            if row["name"].endswith("_at") or "timestamp" in row["name"]]
            clock = [dict(row) for row in connection.execute("SELECT event_id," + ",".join('"' + name + '"' for name in time_columns) + " FROM events ORDER BY event_id")] if time_columns else []
            connection.close()
            results.append({"name": case["name"], "fixture": str(root), "rows": rows,
                            "clock_diagnostics_excluded_from_matching": clock})
    if hashlib.sha256(seed.read_bytes()).hexdigest() != seed_hash:
        raise RuntimeError("seed changed during corpus")
    report_value = {"scenarios": len(results), "api_calls": counts, "seed": str(seed), "seed_sha256": seed_hash,
                    "records": results, "oracle_python": sys.executable, "python_version": sys.version,
                    "sqlite_version": sqlite3.sqlite_version,
                    "oracle_usage_sha256": hashlib.sha256(Path(usage.__file__).read_bytes()).hexdigest(),
                    "generator_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                    "proof_limit": "component SQL/API proof; no open_run, full GraphCase/CLI or core-semantic proof; copied inode metadata is not blessed"}
    (output / "report.json").write_text(json.dumps(report_value, indent=2, ensure_ascii=True) + "\n", encoding="utf-8")
    print(json.dumps({"scenarios": len(results), "api_calls": counts, "total_calls": sum(counts.values())}))


if __name__ == "__main__":
    main()
