"""Registered verification entry points for the native graph experiment."""

import argparse
import json
from pathlib import Path
import sys


sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "parity"))
from workload_io import (
    DAILY, HYGIENE, ORACLE, PACKAGE, WORKTREE, artifact_root, environment, interpreter,
    load_workload, new_output, passed, run_child, run_unittest, verify_oracle, write_json,
)


def python_oracle(output, workload, env):
    frozen_before = verify_oracle(output, "before", workload, ORACLE, env)
    focused = run_unittest(output, "focused", workload.focused_modules, workload.focused, ORACLE, env)
    daily = run_unittest(output, "daily", [DAILY], workload.daily, ORACLE, env)
    frozen_after = verify_oracle(output, "after", workload, ORACLE, env)
    return {
        "frozen_before": frozen_before, "frozen_after": frozen_after,
        "focused": focused, "daily": daily, "separate_hygiene": "pending",
        "passed": focused["passed"] and daily["passed"],
    }


def parity(output, root, env):
    target = Path(env["CARGO_TARGET_DIR"])
    if not target.is_absolute():
        raise ValueError("CARGO_TARGET_DIR must be absolute")
    child = run_child(
        output, "parity-driver",
        [sys.executable, "-B", PACKAGE / "parity/run.py", "--oracle", ORACLE,
         "--rust-bin-dir", target / "release", "--inventory", root / "oracle-inventory.json",
         "--coverage", PACKAGE / "parity/coverage-map.json", "--output", root / "parity"],
        WORKTREE, env,
    )
    return {"driver": child, "passed": passed(child)}


def experiment_debris():
    forbidden_names = {"__pycache__", "target", ".pytest_cache", ".mypy_cache", ".ruff_cache"}
    forbidden_suffixes = (".pyc", ".pyo", ".sqlite3", ".sqlite3-wal", ".sqlite3-shm")
    return sorted(
        path.relative_to(PACKAGE).as_posix()
        for path in PACKAGE.rglob("*")
        if path.name in forbidden_names or path.name.endswith(forbidden_suffixes)
    )


def hygiene(output, workload, env):
    frozen = verify_oracle(output, "before-hygiene", workload, ORACLE, env)
    before = experiment_debris()
    original = run_unittest(output, "original-hygiene", [HYGIENE], workload.hygiene, ORACLE, env)
    after = experiment_debris()
    return {
        "frozen": frozen, "debris_before": before, "debris_after": after,
        "original_hygiene": original, "passed": original["passed"] and not before and not after,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("check", choices=("python-oracle", "parity", "hygiene"))
    check = parser.parse_args().check
    if Path.cwd().resolve() != WORKTREE:
        parser.error("run the registered check from the worktree root")
    try:
        root = artifact_root()
        output = new_output(root / "verification", check, root)
    except (OSError, ValueError, KeyError) as error:
        parser.error(str(error))
    result = {"schema_version": 1, "argv": sys.orig_argv, "check": check, "cwd": str(WORKTREE), "passed": False}
    try:
        env = environment(root)
        workload = load_workload(root / "oracle-inventory.json")
        python = interpreter()
        result.update(inventory_sha256=workload.inventory_sha256, python=python)
        if sys.version_info[:3] != (3, 12, 10) or Path(env["GRAPH_RUST_ORACLE_PYTHON"]).resolve() != Path(python["path"]):
            raise ValueError("registered interpreter differs from the frozen Python 3.12.10 environment")
        if check == "python-oracle":
            result.update(python_oracle(output, workload, env))
        elif check == "parity":
            result.update(parity(output, root, env))
        else:
            result.update(hygiene(output, workload, env))
    except (OSError, ValueError, KeyError) as error:
        result["error"] = str(error)
    report = output / "report.json"
    write_json(report, result)
    print(json.dumps({"check": check, "passed": result["passed"], "report": str(report)}), flush=True)
    return int(not result["passed"])


if __name__ == "__main__":
    raise SystemExit(main())
