"""Run the registered native behavior, private API, and public CLI comparisons."""

import argparse
import json
from pathlib import Path
import shutil
import sys

from public_cli import binary, run_public
from workload_io import (
    DAILY, ORACLE, PACKAGE, WORKTREE, artifact_root, environment, interpreter, load_workload,
    new_output, passed, raw, read_json, require_exit, run_child, sha256, under_root,
    unittest_outcomes, verify_oracle, write_json,
)


API_SCENARIOS = (
    "foundation", "byte_json", "planning", "policy", "policy_loader", "config_private",
    "store", "artifacts", "artifact_files", "sqlite_values", "helper_policy",
)
ORDERED_PACKET_SHA256 = "25ccfa770673a7299f4d78fb7d32401104ef02a5403b515ce9fea38c2eea3f1a"


def native_source():
    suffixes = {".rs", ".toml", ".lock", ".py", ".json", ".sql", ".manifest"}
    return {
        path.relative_to(PACKAGE).as_posix(): sha256(path)
        for path in sorted(PACKAGE.rglob("*")) if path.is_file() and path.suffix in suffixes
    }


def coverage_gaps(coverage):
    unresolved = {"not_started", "partial", "gap"}
    return {
        section: {name: entry["status"] for name, entry in coverage[section].items() if entry["status"] in unresolved}
        for section in ("commands", "modules", "definitions", "mutation_rules")
    }


def oracle_receipt(root, workload, current_python, env):
    candidates = sorted((root / "verification").glob("python-oracle-*"))
    if not candidates:
        raise ValueError("run the registered python-oracle check before parity")
    report_path = under_root(candidates[-1] / "report.json", root)
    report = read_json(report_path)
    if not (
        report.get("passed") and report.get("check") == "python-oracle"
        and report["inventory_sha256"] == workload.inventory_sha256
        and report["python"] == current_python
        and report["frozen_before"]["head"] == report["frozen_after"]["head"] == workload.head
        and report["frozen_before"]["modules"] == report["frozen_after"]["modules"]
    ):
        raise ValueError("latest python-oracle receipt differs from the frozen candidate inputs")
    for stage, names, expected in (
        ("focused", workload.focused_modules, workload.focused), ("daily", [DAILY], workload.daily),
    ):
        result = report[stage]
        record = result["process"]
        if (
            result["observed"] != [[identity, "ok"] for identity in expected] or not result["passed"]
            or record["argv"] != [sys.executable, "-B", "-m", "unittest", "-v", *names]
            or Path(record["cwd"]) != ORACLE
        ):
            raise ValueError(f"original {stage} outcomes differ from the frozen selection")
        for key in ("GRAPH_RUST_RUN_ARTIFACT_ROOT", "TEMP", "TMP", "TMPDIR", "PYTHONDONTWRITEBYTECODE", "PYTHONUNBUFFERED"):
            if record["environment"][key] != env[key]:
                raise ValueError(f"unmatched oracle and native environment for {key}")
        for stream in ("stdout", "stderr"):
            under_root(record[stream], root)
            raw(record, stream)
        if not unittest_outcomes(record, expected)["passed"]:
            raise ValueError(f"raw oracle {stage} output differs from its receipt")
    return {"path": str(report_path), "sha256": sha256(report_path), "behavior": report}


def verify_inventory(workload, coverage):
    rows = coverage["test_identities"]
    if (
        coverage["inventory_sha256"] != workload.inventory_sha256
        or coverage["oracle_revision"] != workload.head
        or set(rows) != set(workload.behavior + workload.hygiene)
    ):
        raise ValueError("coverage identity set differs from the frozen workload")
    if any(rows[identity]["status"] != "proven" for identity in workload.behavior):
        raise ValueError("an original behavior identity lacks an audited native proof")
    if any(rows[identity].get("native_dispatch") for identity in workload.hygiene):
        raise ValueError("hygiene must remain a separate original Python process")
    selectors = {
        (entry["native_dispatch"]["target"], entry["native_dispatch"]["selector"])
        for entry in rows.values() if entry.get("native_dispatch")
    }
    if len(selectors) != 234:
        raise ValueError("audited native selectors are missing or aliased")
    modules = {Path(entry["path"]).stem: entry for entry in workload.modules}
    definitions = {
        f"{module}.{definition['name']}": (entry, [definition["line"], definition["end_line"]])
        for module, entry in modules.items() for definition in entry["definitions"]
    }
    if (
        set(coverage["modules"]) != set(modules)
        or set(coverage["definitions"]) != set(definitions)
        or {name: row["oracle_options"] for name, row in coverage["commands"].items()} != workload.command_options
        or len(coverage["mutation_rules"]) != 25
    ):
        raise ValueError("coverage map omits or changes a frozen command, module, definition, or mutation")
    for name, (entry, lines) in definitions.items():
        row = coverage["definitions"][name]
        if (row["oracle_path"], row["oracle_sha256"], row["oracle_lines"]) != (entry["path"], entry["sha256"], lines):
            raise ValueError(f"frozen definition source binding differs for {name}")
    return sorted({target for target, _ in selectors if target != "graph_engine"})


def verify_order(root, workload):
    path = under_root(root / "runner-frozen-identity-inputs-02.json", root)
    if sha256(path) != ORDERED_PACKET_SHA256:
        raise ValueError("ordered workload packet changed")
    order = read_json(path)
    if (
        order["inventory_sha256"] != workload.inventory_sha256
        or tuple(order["ordinal_behavior_ids"]) != workload.behavior
        or tuple(order["sequential"]["focused"]) != workload.focused
        or tuple(order["sequential"]["daily"]) != workload.daily
        or (order["hygiene"],) != workload.hygiene
    ):
        raise ValueError("ordered workload selections differ from the frozen inventory")
    expected = [list(workload.behavior[index::4]) for index in range(4)]
    if [worker["identities"] for worker in order["four_worker"]] != expected:
        raise ValueError("four-worker partitions differ from the frozen ordinal partition")
    return ORDERED_PACKET_SHA256


def discover_release(output, native, workload, coverage_path, targets, env):
    manifest_path = PACKAGE.relative_to(WORKTREE) / "Cargo.toml"
    test_argv = ["cargo", "test", "--release", "--locked", "--manifest-path", manifest_path,
                 "--no-run", "--message-format=json", "--lib"]
    for target in targets:
        test_argv.extend(("--test", target))
    cargo = run_child(output, "release-executables", test_argv, WORKTREE, env, timeout=1800)
    require_exit(cargo)
    suite = binary(native, "graph-suite")
    registry = run_child(output, "native-registry", [suite, "--list"], WORKTREE, env, timeout=60)
    require_exit(registry)
    actual = tuple(raw(registry, "stdout").decode("utf-8").splitlines())
    if actual != workload.behavior or raw(registry, "stderr"):
        raise ValueError("release registry differs from the frozen behavior inventory")
    discovered = run_child(output, "native-discovery", [suite, "--discover-tests", cargo["stdout"]], WORKTREE, env, timeout=120)
    require_exit(discovered)
    manifest = json.loads(raw(discovered, "stdout"))
    if not (
        manifest["registry_sha256"] == sha256(coverage_path)
        and manifest["cargo_log_sha256"] == cargo["stdout_sha256"]
        and len(manifest["binaries"]) == len(targets) + 1
    ):
        raise ValueError("release binary manifest differs from the authored registry or Cargo output")
    executables = manifest["binaries"] | manifest["auxiliary"]
    for name in ("graph-parity",):
        executable = binary(native, name).resolve(strict=True)
        executables[name] = {"path": str(executable), "sha256": sha256(executable)}
    for entry in executables.values():
        path = Path(entry["path"]).resolve(strict=True)
        if not path.is_relative_to(native) or sha256(path) != entry["sha256"]:
            raise ValueError("release executable is outside the selected release directory or its digest changed")
    return {
        "cargo": cargo, "registry": registry, "discovery": discovered, "executables": executables,
        "identity_count": len(actual),
    }


def native_outcomes(process, worker, expected, manifest_sha):
    stream = raw(process, "stdout").decode("utf-8", errors="replace")
    passed_ids = [line[5:] for line in stream.splitlines() if line.startswith("PASS ")]
    observed = []
    boundary_passed = True
    for boundary in worker["boundaries"]:
        if boundary["mode"] == "inline":
            observed.append(boundary["identity"])
            boundary_passed &= boundary["passed"]
        elif boundary["mode"] == "libtest":
            observed.extend(boundary["identities"])
            boundary_passed &= (
                boundary["exact_order"] and boundary["process"]["exit_code"] == 0
                and len(boundary["observed"]) == len(boundary["identities"])
                and all(item["outcome"] == "ok" for item in boundary["observed"])
            )
        else:
            raise ValueError("unexpected native worker execution boundary")
    return {
        "expected": list(expected), "observed": observed, "passed_ids": passed_ids,
        "passed": passed(process) and worker["passed"] and boundary_passed
        and worker["test_binaries_sha256"] == manifest_sha
        and list(expected) == passed_ids == observed == worker["identities"],
    }


def native_stage(output, label, identities, suite, env, manifest_sha):
    selection = output / f"{label}.list"
    with selection.open("x", encoding="utf-8", newline="\n") as stream:
        stream.write("\n".join(identities) + "\n")
    path = output / f"{label}-worker.json"
    process = run_child(
        output, label, [suite, "--selection", selection, "--report", path],
        WORKTREE, env, timeout=1800,
    )
    worker = read_json(path) if path.is_file() and path.stat().st_size else None
    outcome = native_outcomes(process, worker, identities, manifest_sha) if worker is not None else {"passed": False}
    return {
        "process": process, "selection_sha256": sha256(selection),
        "worker": str(path), "worker_sha256": sha256(path) if worker is not None else None,
        **outcome,
    }


def run_api(output, oracle, native, env):
    output.mkdir()
    probe = binary(native, "graph-parity")
    comparisons = {}
    for label in API_SCENARIOS:
        script = PACKAGE / "parity" / f"{label}.py"
        arguments = [] if label in {"byte_json", "sqlite_values"} else ["--oracle", oracle]
        directory = output / label
        child = run_child(
            output, label, [sys.executable, "-B", script, *arguments, "--rust", probe, "--output", directory],
            WORKTREE, env, timeout=300,
        )
        report = directory / "report.json"
        comparisons[label] = {
            "process": child, "tool_sha256": sha256(script),
            "report_sha256": sha256(report) if report.is_file() else None,
            "passed": passed(child) and report.is_file(),
        }

    # The helper comparison confines its executable to the artifact root.
    helper = binary(native, "helper-register")
    copy = binary(output, "helper-register")
    shutil.copyfile(helper, copy)
    if sha256(copy) != sha256(helper):
        raise ValueError("artifact-owned helper executable copy differs")
    script = PACKAGE / "parity/helper_cli.py"
    child = run_child(
        output, "helper-cli",
        [sys.executable, "-B", script, "--native", copy, "--output", output / "helper-cli"],
        WORKTREE, env, timeout=600,
    )
    report = output / "helper-cli/report.json"
    comparisons["helper-cli"] = {
        "process": child, "tool_sha256": sha256(script), "binary_sha256": sha256(copy),
        "report_sha256": sha256(report) if report.is_file() else None,
        "passed": passed(child) and report.is_file(),
    }

    negative = PACKAGE / "tools/verify_frozen_inputs.py"
    child = run_child(
        output, "frozen-inputs",
        [sys.executable, "-B", negative, "--output", output / "frozen-inputs"],
        WORKTREE, env, timeout=1200,
    )
    report = output / "frozen-inputs/report.json"
    comparisons["frozen-inputs"] = {
        "process": child, "tool_sha256": sha256(negative),
        "report_sha256": sha256(report) if report.is_file() else None,
        "passed": passed(child) and report.is_file(),
    }
    return comparisons


def run(args, root, output):
    oracle = args.oracle.resolve(strict=True)
    coverage_path = args.coverage.resolve(strict=True)
    native = args.rust_bin_dir.resolve()
    env = environment(root)
    target = Path(env["CARGO_TARGET_DIR"])
    if (
        oracle != ORACLE or coverage_path != PACKAGE / "parity/coverage-map.json"
        or not target.is_absolute() or native != (target / "release").resolve()
    ):
        raise ValueError("oracle, coverage, and release binaries must use the approved boundaries")
    inventory = under_root(args.inventory, root)
    workload = load_workload(inventory)
    ordered_sha = verify_order(root, workload)
    coverage = read_json(coverage_path)
    targets = verify_inventory(workload, coverage)
    before = native_source()
    for identity, entry in coverage["test_identities"].items():
        if entry.get("native_dispatch") and before.get(entry["native_component"]) != entry["native_source_sha256"]:
            raise ValueError(f"audited native case source changed for {identity}")
    python = interpreter()
    original = oracle_receipt(root, workload, python, env)
    frozen_before = verify_oracle(output, "parity-before", workload, oracle, env)
    toolchain = [
        run_child(output, name, argv, WORKTREE, env, timeout=60)
        for name, argv in (("rustc", ["rustc", "--version", "--verbose"]), ("cargo", ["cargo", "--version"]))
    ]
    for record in toolchain:
        require_exit(record)
    release = discover_release(output, native, workload, coverage_path, targets, env)
    manifest = release["discovery"]
    env["GRAPH_RUST_TEST_BINARIES"] = manifest["stdout"]
    env["GRAPH_RUST_HELPER_BINARY"] = release["executables"]["helper-register"]["path"]
    suite = binary(native, "graph-suite")
    behaviors = {
        name: native_stage(output, f"native-{name}", identities, suite, env, manifest["stdout_sha256"])
        for name, identities in (("focused", workload.focused), ("daily", workload.daily))
    }
    api = run_api(output / "api", oracle, native, env)
    public = run_public(output / "public", root, oracle, native, coverage, env)
    frozen_after = verify_oracle(output, "parity-after", workload, oracle, env)
    stable = before == native_source()
    stable &= all(sha256(record["path"]) == record["sha256"] for record in release["executables"].values())
    gaps = coverage_gaps(coverage)
    result = {
        "inventory_sha256": workload.inventory_sha256, "ordered_packet_sha256": ordered_sha,
        "coverage_sha256": sha256(coverage_path),
        "oracle": original, "oracle_before": frozen_before, "oracle_after": frozen_after,
        "native_source": before, "stable_source_and_binaries": stable, "toolchain": toolchain,
        "python": python, "release": release, "native_behavior": behaviors,
        "api_comparisons": api, "public_cli": public, "unresolved_coverage": gaps,
        "separate_hygiene": "pending separate registered check",
        "passed": stable and all(stage["passed"] for stage in behaviors.values())
        and all(stage["passed"] for stage in api.values()) and public["passed"]
        and not any(gaps.values()),
    }
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--oracle", type=Path, required=True)
    parser.add_argument("--rust-bin-dir", type=Path, required=True)
    parser.add_argument("--inventory", type=Path, required=True)
    parser.add_argument("--coverage", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        root = artifact_root()
        output = new_output(args.output, "registered-parity", root)
    except (OSError, ValueError, KeyError) as error:
        parser.error(str(error))
    result = {"schema_version": 1, "argv": sys.orig_argv, "passed": False}
    try:
        result.update(run(args, root, output))
    except (OSError, ValueError, KeyError) as error:
        result["error"] = str(error)
    report = output / "report.json"
    write_json(report, result)
    print(json.dumps({"passed": result["passed"], "report": str(report)}), flush=True)
    return int(not result["passed"])


if __name__ == "__main__":
    raise SystemExit(main())
