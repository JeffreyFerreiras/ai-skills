"""Measure the frozen Python and native release workloads after verified parity."""

import argparse
from concurrent.futures import ThreadPoolExecutor
from dataclasses import asdict, dataclass
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import statistics
import sys
import time

from benchmark_costs import cost_plan
from public_cli import GIT_OVERRIDES, ROOT_OVERRIDES, binary
from run import native_outcomes, native_source, verify_inventory, verify_order
from workload_io import (
    DAILY, ORACLE, PACKAGE, WORKTREE, artifact_root, bind_child, capture_child,
    environment, interpreter, load_workload, passed, raw, read_json, recorded_environment,
    require_exit, run_child, sha256, under_root, unittest_outcomes, verify_oracle, write_json,
)


CONFIGURATIONS = ("sequential", "four-workers")
LANGUAGES = ("python", "rust")
FIXTURE_INPUTS = {
    "oracle-inventory.json": "96d0b42a520e4578a847caf4721e033d65143f439a8370edaa1e044b3625af16",
    "parity/datetime-01/report.json": "d0cf30485a1eace8c7c23cda9c4d113c4a1e9332ef13f1db5fe23e84c3a802a2",
    "compat/utc-after-reference-01/stdout.bin": "723fcd7e700bbd26375092c71ae72028c1f1092caf3fb306c108da87a8b5ee0c",
    "parity/source-stat-01/report.json": "ac45ab0fbee24a661dddc686438a363a519e1338d36d30c267a9780536945787",
}
EXCLUSIVE_WINDOW = {"agents": 0, "builds": 0, "competing_benchmarks": 0}


@dataclass(frozen=True)
class Trial:
    ordinal: int
    sample: int
    configuration: str
    language: str


@dataclass(frozen=True)
class Stage:
    label: str
    worker: str
    names: tuple
    identities: tuple


@dataclass(frozen=True)
class PreparedStage:
    stage: Stage
    argv: tuple
    cwd: Path
    env: dict
    selection: Path | None = None
    worker_report: Path | None = None


def schedule(trials, configurations):
    if trials < 3:
        raise ValueError("at least three matched trials per configuration are required")
    if len(configurations) != 2 or set(configurations) != set(CONFIGURATIONS):
        raise ValueError("select each approved configuration exactly once")
    result = []
    for sample in range(1, trials + 1):
        order = configurations if sample % 2 else reversed(configurations)
        for configuration in order:
            first = (sample - 1 + CONFIGURATIONS.index(configuration)) % 2
            for language in (LANGUAGES[first], LANGUAGES[1 - first]):
                result.append(Trial(len(result) + 1, sample, configuration, language))
    return result


def behavior_stages(workload, configuration):
    if configuration == "sequential":
        return (
            Stage("focused", "w0", workload.focused_modules, workload.focused),
            Stage("daily", "w0", (DAILY,), workload.daily),
        )
    if configuration == "four-workers":
        partitions = tuple(workload.behavior[index::4] for index in range(4))
        return tuple(
            Stage(f"worker-{index}", f"w{index}", identities, identities)
            for index, identities in enumerate(partitions)
        )
    raise ValueError(f"unknown configuration: {configuration}")


def plan(workload, trials, oracle, suite, order_sha, coverage_sha):
    return {
        "schema_version": 1,
        "inventory_sha256": workload.inventory_sha256,
        "ordered_packet_sha256": order_sha,
        "coverage_sha256": coverage_sha,
        "trial_order": [asdict(trial) for trial in trials],
        "behavior": {
            configuration: [asdict(stage) for stage in behavior_stages(workload, configuration)]
            for configuration in CONFIGURATIONS
        },
        "hygiene": {"names": workload.hygiene, "last": True, "language": "python"},
        "fixture_seed_sha256": FIXTURE_INPUTS,
        "release_runner": str(suite),
        "conditions_contract": {
            "schema_version": 1, "exclusive_window": EXCLUSIVE_WINDOW,
            "required_text": ["recorded_at", "machine", "storage", "power", "cache"],
        },
        "cost_measurements": cost_plan(sys.executable, oracle, suite),
        "wall_boundaries": {
            "behavior": "From worker dispatch to the end of all behavior subprocesses, including scheduling and raw redirection.",
            "complete_trial": "Behavior through the separate original hygiene process.",
            "fixture_preparation": "Isolated roots, bound seed copies and selection files before behavior timing.",
            "evidence_binding": "Post-run stream digests, receipts and outcome validation, outside the measured process window.",
            "native": "Worker reports retain inline cases and libtest launch/collection boundaries. Child walls are not subtracted from overlapping trial wall.",
        },
    }


def fixture_seeds(root):
    result = {}
    for name, expected in FIXTURE_INPUTS.items():
        source = under_root(root / name, root)
        data = source.read_bytes()
        if hashlib.sha256(data).hexdigest() != expected:
            raise ValueError(f"benchmark fixture input changed: {name}")
        result[name] = data
    return result


def fixture_environment(scope, seeds, env):
    scope.mkdir()
    temp = scope / "fixtures"
    temp.mkdir()
    for name, data in seeds.items():
        target = scope / name
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open("xb") as stream:
            stream.write(data)
    return dict(
        env, GRAPH_RUST_RUN_ARTIFACT_ROOT=str(scope), TEMP=str(temp), TMP=str(temp),
        TMPDIR=str(temp), NPM_CONFIG_CACHE=str(scope / "npm"),
    )


def prepare_trial(output, fixture_root, trial, workload, suite, env, native_env, seeds):
    behavior = behavior_stages(workload, trial.configuration)
    hygiene = Stage("hygiene", "h", workload.hygiene, workload.hygiene)
    trial_root = fixture_root / f"{trial.ordinal:03}"
    trial_root.mkdir()
    workers = {
        worker: fixture_environment(trial_root / worker, seeds, env)
        for worker in dict.fromkeys(stage.worker for stage in (*behavior, hygiene))
    }
    prepared = []
    for stage in (*behavior, hygiene):
        child_env = workers[stage.worker]
        if trial.language == "python" or stage is hygiene:
            argv = (sys.executable, "-B", "-m", "unittest", "-v", *stage.names)
            prepared.append(PreparedStage(stage, argv, ORACLE, child_env))
            continue
        selection = output / f"{stage.label}.list"
        with selection.open("x", encoding="utf-8", newline="\n") as stream:
            stream.write("\n".join(stage.identities) + "\n")
        worker_report = output / f"{stage.label}-worker.json"
        argv = (suite, "--selection", selection, "--report", worker_report)
        prepared.append(PreparedStage(stage, argv, WORKTREE, child_env | native_env, selection, worker_report))
    return tuple(prepared), {
        worker: {"root": worker_env["GRAPH_RUST_RUN_ARTIFACT_ROOT"], "environment": recorded_environment(worker_env)}
        for worker, worker_env in workers.items()
    }


def collect_trial(output, configuration, prepared):
    def capture(stage):
        return capture_child(output, stage.stage.label, stage.argv, stage.cwd, stage.env)

    behavior, hygiene = prepared[:-1], prepared[-1]
    started = time.perf_counter_ns()
    if configuration == "four-workers":
        with ThreadPoolExecutor(max_workers=4) as executor:
            captured = list(executor.map(capture, behavior))
    else:
        captured = [capture(stage) for stage in behavior]
    behavior_finished = time.perf_counter_ns()
    captured.append(capture(hygiene))
    finished = time.perf_counter_ns()
    if captured[-1]["started_monotonic_ns"] < max(record["finished_monotonic_ns"] for record in captured[:-1]):
        raise ValueError("hygiene started before behavior workers exited")
    return captured, {
        "behavior_wall_ns": behavior_finished - started,
        "complete_trial_wall_ns": finished - started,
        "hygiene_process_ns": captured[-1]["elapsed_ns"],
    }


def stage_outcome(output, prepared, captured, manifest_sha, coverage_sha, dispatched):
    process = bind_child(output, captured)
    identities = prepared.stage.identities
    if prepared.worker_report is None:
        return {"process": process, **unittest_outcomes(process, identities)}
    worker_path = prepared.worker_report
    outcome = {"process": process, "passed": False, "expected": list(identities),
               "selection_sha256": sha256(prepared.selection), "worker": str(worker_path)}
    try:
        worker = read_json(worker_path)
        expected_manifest = manifest_sha if any(identity in dispatched for identity in identities) else None
        result = native_outcomes(process, worker, identities, expected_manifest)
        outcome.update(result, worker_sha256=sha256(worker_path))
        outcome["passed"] &= worker["registry_sha256"] == coverage_sha
        outcome["inline_cases"] = sum(boundary["mode"] == "inline" for boundary in worker["boundaries"])
        outcome["libtest_child_processes"] = sum(boundary["mode"] == "libtest" for boundary in worker["boundaries"])
    except (OSError, ValueError, KeyError, TypeError) as error:
        outcome.update(passed=False, evidence_error=str(error))
    return outcome


def run_trial(output, fixture_root, trial, workload, suite, env, native_env, seeds,
              manifest_sha, coverage_sha, dispatched):
    trial_output = output / f"{trial.ordinal:03}-{trial.language}-{trial.configuration}"
    trial_output.mkdir()
    fixture_started = time.perf_counter_ns()
    prepared, workers = prepare_trial(trial_output, fixture_root, trial, workload, suite, env, native_env, seeds)
    fixture_ns = time.perf_counter_ns() - fixture_started
    captured, timing = collect_trial(trial_output, trial.configuration, prepared)
    evidence_started = time.perf_counter_ns()
    outcomes = [
        stage_outcome(trial_output, stage, child, manifest_sha, coverage_sha, dispatched)
        for stage, child in zip(prepared, captured, strict=True)
    ]
    timing.update(fixture_preparation_ns=fixture_ns, evidence_binding_ns=time.perf_counter_ns() - evidence_started)
    result = {
        **asdict(trial), "timing": timing, "workers": workers, "behavior": outcomes[:-1], "hygiene": outcomes[-1],
        "behavior_workers": 1 if trial.configuration == "sequential" else 4,
        "passed": all(outcome["passed"] for outcome in outcomes),
    }
    write_json(trial_output / "trial.json", result)
    return result


def stats(values):
    return {
        "n": len(values), "observations_ns": values,
        "median_ns": statistics.median(values) if values else None,
        "min_ns": min(values, default=None), "max_ns": max(values, default=None),
        "sample_sd_ns": statistics.stdev(values) if len(values) > 1 else None,
    }


def summarize(results, planned_count):
    summary = {}
    for configuration in CONFIGURATIONS:
        comparisons = {}
        groups = {
            language: [row for row in results if row["configuration"] == configuration and row["language"] == language]
            for language in LANGUAGES
        }
        complete = all(len(rows) == planned_count and all(row["passed"] for row in rows) for rows in groups.values())
        for metric in ("behavior_wall_ns", "complete_trial_wall_ns", "hygiene_process_ns",
                       "fixture_preparation_ns", "evidence_binding_ns"):
            measurements = {language: stats([row["timing"][metric] for row in rows]) for language, rows in groups.items()}
            if complete and metric in {"behavior_wall_ns", "complete_trial_wall_ns"}:
                python = measurements["python"]["median_ns"]
                rust = measurements["rust"]["median_ns"]
                measurements["python_over_rust_median_ratio"] = python / rust if rust else None
            comparisons[metric] = measurements
        summary[configuration] = {"complete": complete, "measurements": comparisons}
    return summary


def measurement_conditions(path, root):
    path = under_root(path, root)
    contents = path.read_bytes()
    conditions = json.loads(contents)
    if conditions.get("schema_version") != 1 or conditions.get("exclusive_window") != EXCLUSIVE_WINDOW:
        raise ValueError("measurement conditions must record an exclusive timing window")
    for field in ("recorded_at", "machine", "storage", "power", "cache"):
        if not isinstance(conditions.get(field), str) or not conditions[field].strip():
            raise ValueError(f"measurement conditions missing {field}")
    return {"path": str(path), "sha256": hashlib.sha256(contents).hexdigest(), "declared": conditions}


def executable_hashes(executables, native):
    bindings = {}
    for name, entry in executables.items():
        path = Path(entry["path"]).resolve(strict=True)
        if not path.is_relative_to(native) or sha256(path) != entry["sha256"]:
            raise ValueError(f"parity release executable changed: {name}")
        bindings[name] = {"path": str(path), "sha256": entry["sha256"]}
    return bindings


def matched_parity(root, workload, order_sha, coverage_sha, suite, python, source):
    candidates = sorted((root / "parity").glob("registered-parity-*"))
    if not candidates:
        raise ValueError("a passing registered parity report is required before measurements")
    path = under_root(candidates[-1] / "report.json", root)
    contents = path.read_bytes()
    report = json.loads(contents)
    if not (
        report.get("passed") and report.get("stable_source_and_binaries")
        and report["inventory_sha256"] == workload.inventory_sha256
        and report["ordered_packet_sha256"] == order_sha and report["coverage_sha256"] == coverage_sha
        and report["python"] == python and report["native_source"] == source
        and not any(report["unresolved_coverage"].values())
    ):
        raise ValueError("latest parity report does not bind the current complete candidate")
    for name, identities in (("focused", workload.focused), ("daily", workload.daily)):
        stage = report["native_behavior"][name]
        if not stage["passed"] or stage["observed"] != list(identities):
            raise ValueError(f"registered parity stage changed: {name}")
    release = report["release"]
    executables = executable_hashes(release["executables"], suite.parent)
    if Path(executables["graph-suite"]["path"]) != suite:
        raise ValueError("selected graph-suite differs from the parity-bound release executable")
    discovery = release["discovery"]
    require_exit(discovery)
    manifest_path = under_root(discovery["stdout"], root)
    manifest = json.loads(raw(discovery, "stdout"))
    declared = manifest["binaries"] | manifest["auxiliary"]
    if (manifest["registry_sha256"] != coverage_sha or set(executables) != set(declared) | {"graph-parity"}
            or any(executables[name]["sha256"] != entry["sha256"]
                   or Path(executables[name]["path"]) != Path(entry["path"]).resolve(strict=True)
                   for name, entry in declared.items())):
        raise ValueError("release discovery and the parity executable bindings differ")
    for record in report["toolchain"]:
        require_exit(record)
        under_root(record["stdout"], root)
        under_root(record["stderr"], root)
        raw(record, "stdout")
        raw(record, "stderr")
    return {
        "report": {"path": str(path), "sha256": hashlib.sha256(contents).hexdigest()},
        "manifest": {"path": str(manifest_path), "sha256": discovery["stdout_sha256"]},
        "executables": executables, "toolchain": report["toolchain"],
    }


def measure(args, root, output, workload, trials, suite, order_sha, coverage, coverage_sha, result):
    env = environment(root)
    for name in (*GIT_OVERRIDES, *ROOT_OVERRIDES, "GRAPH_RUST_TEST_BINARIES", "GRAPH_RUST_HELPER_BINARY"):
        env.pop(name, None)
    python = interpreter()
    if sys.version_info[:3] != (3, 12, 10) or Path(env["GRAPH_RUST_ORACLE_PYTHON"]).resolve() != Path(python["path"]):
        raise ValueError("measurement interpreter differs from frozen Python 3.12.10")
    conditions = measurement_conditions(args.conditions or root / "measurements/conditions.json", root)
    if any(entry["status"] in {"not_started", "partial", "gap"}
           for section in ("commands", "modules", "definitions", "mutation_rules") for entry in coverage[section].values()):
        raise ValueError("close all coverage gaps before measuring")
    source = native_source()
    release = matched_parity(root, workload, order_sha, coverage_sha, suite, python, source)
    tools = {}
    for name in ("git", "cargo", "rustc"):
        selected = shutil.which(name)
        if not selected:
            raise ValueError(f"{name} executable is required")
        executable = Path(selected).resolve(strict=True)
        tools[name] = {"path": str(executable), "sha256": sha256(executable)}
    git_version = run_child(output, "git-version", [tools["git"]["path"], "--version"], WORKTREE, env, timeout=60)
    require_exit(git_version)
    frozen_before = verify_oracle(output, "measurement-before", workload, ORACLE, env)
    seeds = fixture_seeds(root)
    fixture_parent = under_root(root / "b", root)
    fixture_parent.mkdir(exist_ok=True)
    fixture_root = fixture_parent / hashlib.sha256(os.fsencode(output)).hexdigest()[:12]
    fixture_root.mkdir()
    native_env = {
        "GRAPH_RUST_TEST_BINARIES": release["manifest"]["path"],
        "GRAPH_RUST_HELPER_BINARY": release["executables"]["helper-register"]["path"],
    }
    result.update(
        conditions=conditions, platform=platform.platform(), processor=platform.processor(),
        logical_processors=os.cpu_count(), python=python, tools=tools, git_version=git_version,
        frozen_before=frozen_before, source_before=source, release=release, fixture_root=str(fixture_root),
        fixture_seed_sha256=FIXTURE_INPUTS,
    )
    dispatched = {identity for identity, entry in coverage["test_identities"].items() if entry.get("native_dispatch")}
    for trial in trials:
        row = run_trial(output, fixture_root, trial, workload, suite, env, native_env, seeds,
                        release["manifest"]["sha256"], coverage_sha, dispatched)
        result["trials"].append(row)
        print(json.dumps({"trial": asdict(trial), "timing": row["timing"], "passed": row["passed"]}), flush=True)
        if not row["passed"]:
            break
    frozen_after = verify_oracle(output, "measurement-after", workload, ORACLE, env)
    stable = (source == native_source() and frozen_before["modules"] == frozen_after["modules"]
              and executable_hashes(release["executables"], suite.parent) == release["executables"]
              and sha256(release["manifest"]["path"]) == release["manifest"]["sha256"])
    result.update(
        frozen_after=frozen_after, stable_source_and_binaries=stable,
        passed=stable and len(result["trials"]) == len(trials) and all(row["passed"] for row in result["trials"]),
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--oracle", type=Path, required=True)
    parser.add_argument("--rust-suite", type=Path, required=True)
    parser.add_argument("--inventory", type=Path, required=True)
    parser.add_argument("--trials", type=int, default=3)
    parser.add_argument("--config", action="append", choices=CONFIGURATIONS)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--conditions", type=Path)
    parser.add_argument("--plan", action="store_true", help="write the frozen plan without launching any process")
    args = parser.parse_args()
    try:
        root = artifact_root()
        target = Path(os.environ["CARGO_TARGET_DIR"])
        suite = args.rust_suite.resolve()
        if (Path.cwd().resolve() != WORKTREE or args.oracle.resolve(strict=True) != ORACLE
                or not target.is_absolute() or suite != binary(target / "release", "graph-suite").resolve()):
            raise ValueError("use the approved worktree, frozen oracle and release suite")
        trials = schedule(args.trials, args.config or CONFIGURATIONS)
        workload = load_workload(under_root(args.inventory, root))
        order_sha = verify_order(root, workload)
        coverage_path = PACKAGE / "parity/coverage-map.json"
        coverage_bytes = coverage_path.read_bytes()
        coverage = json.loads(coverage_bytes)
        verify_inventory(workload, coverage)
        coverage_sha = hashlib.sha256(coverage_bytes).hexdigest()
        output = under_root(args.output, root)
        if output == root:
            raise ValueError("choose a new measurement directory below the artifact root")
        output.mkdir(parents=True)
    except (OSError, ValueError, KeyError) as error:
        parser.error(str(error))
    write_json(output / "plan.json", plan(workload, trials, ORACLE, suite, order_sha, coverage_sha))
    if args.plan:
        print(json.dumps({"mode": "plan", "processes": 0, "plan": str(output / "plan.json")}), flush=True)
        return 0
    result = {"schema_version": 1, "argv": sys.orig_argv, "cwd": str(WORKTREE),
              "plan_sha256": sha256(output / "plan.json"), "trials": [], "passed": False}
    try:
        measure(args, root, output, workload, trials, suite, order_sha, coverage, coverage_sha, result)
    except (OSError, ValueError, KeyError, TypeError) as error:
        result["error"] = str(error)
    if result["trials"]:
        result["statistics"] = summarize(result["trials"], args.trials)
        if not result["passed"]:
            for configuration in result["statistics"].values():
                for measurements in configuration["measurements"].values():
                    measurements.pop("python_over_rust_median_ratio", None)
    path = output / "report.json"
    write_json(path, result)
    print(json.dumps({"mode": "measure", "passed": result["passed"], "report": str(path)}), flush=True)
    return int(not result["passed"])


if __name__ == "__main__":
    raise SystemExit(main())
