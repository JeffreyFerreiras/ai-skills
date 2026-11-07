"""Frozen workload selection and source-bound subprocess receipts."""

from dataclasses import dataclass
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import unicodedata


PACKAGE = Path(__file__).resolve().parent.parent
WORKTREE = PACKAGE.parent.parent
ORACLE = WORKTREE / "skills/software-engineering-graph"
INVENTORY_SHA256 = "96d0b42a520e4578a847caf4721e033d65143f439a8370edaa1e044b3625af16"
DAILY = "tests.test_daily_workflow"
HYGIENE = "tests.test_standalone_acceptance.StandaloneAcceptanceTests.test_hygiene"
ENVIRONMENT_KEYS = (
    "GRAPH_RUST_RUN_ARTIFACT_ROOT", "GRAPH_RUST_ORACLE_PYTHON",
    "GRAPH_RUST_TEST_BINARIES", "GRAPH_RUST_HELPER_BINARY",
    "CARGO_HOME", "CARGO_TARGET_DIR", "TEMP", "TMP", "TMPDIR",
    "PYTHONDONTWRITEBYTECODE", "PYTHONUNBUFFERED", "PYTHONUTF8",
    "PYTHONIOENCODING", "COLUMNS", "NPM_CONFIG_CACHE",
)
TEST_ROW = re.compile(r"^test_\S+ \((tests\.[^)\s]+)\) \.\.\. (.+)$", re.MULTILINE)
TEST_COUNT = re.compile(r"^Ran (\d+) tests? in ([\d.]+)s$", re.MULTILINE)


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write_json(path, value):
    with Path(path).open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(value, stream, ensure_ascii=True, indent=2)
        stream.write("\n")


def read_json(path):
    return json.loads(Path(path).read_bytes())


def artifact_root():
    path = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"])
    if not path.is_absolute():
        raise ValueError("GRAPH_RUST_RUN_ARTIFACT_ROOT must be absolute")
    return path.resolve(strict=True)


def under_root(path, root):
    resolved = Path(path).resolve()
    if not resolved.is_relative_to(root):
        raise ValueError(f"path is outside the run artifact root: {resolved}")
    return resolved


def new_output(parent, stem, root):
    parent = under_root(parent, root)
    parent.mkdir(parents=True, exist_ok=True)
    for ordinal in range(1, 1000):
        output = parent / f"{stem}-{ordinal:03}"
        try:
            output.mkdir()
        except FileExistsError:
            continue
        return output
    raise ValueError(f"no unused receipt directory for {stem}")


def environment(root):
    fixtures = under_root(root / "fixtures", root)
    fixtures.mkdir(exist_ok=True)
    return dict(
        os.environ,
        GRAPH_RUST_RUN_ARTIFACT_ROOT=str(root),
        GRAPH_RUST_ORACLE_PYTHON=os.environ.get("GRAPH_RUST_ORACLE_PYTHON", sys.executable),
        TEMP=str(fixtures),
        TMP=str(fixtures),
        TMPDIR=str(fixtures),
        PYTHONDONTWRITEBYTECODE="1",
        PYTHONUNBUFFERED="1",
    )


def recorded_environment(env):
    return {name: env.get(name) for name in ENVIRONMENT_KEYS}


def interpreter():
    return {
        "path": str(Path(sys.executable).resolve(strict=True)),
        "sha256": sha256(sys.executable),
        "version": sys.version,
        "unicode_version": unicodedata.unidata_version,
        "recursion_limit": sys.getrecursionlimit(),
        "integer_digit_limit": sys.get_int_max_str_digits(),
    }


@dataclass(frozen=True)
class Workload:
    inventory_sha256: str
    head: str
    modules: tuple
    command_options: dict
    focused_modules: tuple
    focused: tuple
    daily: tuple
    hygiene: tuple

    @property
    def behavior(self):
        return tuple(sorted(self.focused + self.daily))


def load_workload(inventory):
    contents = Path(inventory).read_bytes()
    digest = hashlib.sha256(contents).hexdigest()
    if digest != INVENTORY_SHA256:
        raise ValueError("frozen inventory digest differs")
    frozen = json.loads(contents)
    names = tuple(frozen["focused_modules"])
    groups = frozen["test_groups"]
    commands = {
        f"{family} {leaf['command']}": leaf["options"]
        for field, family in (("graph_cli_leaves", "graphctl"), ("helper_register_cli_leaves", "helper-register"))
        for leaf in frozen[field]
    }
    workload = Workload(
        digest, frozen["head"], tuple(frozen["modules"]), commands, names,
        tuple(identity for name in names for identity in groups[name]),
        tuple(groups[DAILY]), tuple(groups[HYGIENE]),
    )
    if (len(names), len(workload.focused), len(workload.daily), workload.hygiene) != (8, 346, 13, (HYGIENE,)):
        raise ValueError("frozen workload groups differ")
    if len(set(workload.behavior + workload.hygiene)) != 360:
        raise ValueError("frozen workload identities are duplicated")
    return workload


def capture_child(output, label, argv, cwd, env, timeout=3600):
    command = [os.fspath(argument) for argument in argv]
    stdout = output / f"{label}.stdout"
    stderr = output / f"{label}.stderr"
    started_at = datetime.now(timezone.utc).isoformat()
    started = time.perf_counter_ns()
    process_id = None
    exit_code = None
    timed_out = False
    launch_error = None
    with stdout.open("xb") as out_stream, stderr.open("xb") as err_stream:
        try:
            with subprocess.Popen(command, cwd=cwd, env=env, stdout=out_stream, stderr=err_stream) as process:
                process_id = process.pid
                try:
                    exit_code = process.wait(timeout=timeout)
                except subprocess.TimeoutExpired:
                    timed_out = True
                    process.kill()
                    exit_code = process.wait()
        except OSError as error:
            launch_error = str(error)
    finished_at = datetime.now(timezone.utc).isoformat()
    finished = time.perf_counter_ns()
    return {
        "label": label, "argv": command, "cwd": str(cwd),
        "environment": recorded_environment(env), "pid": process_id,
        "started_at": started_at, "finished_at": finished_at,
        "started_monotonic_ns": started, "finished_monotonic_ns": finished,
        "elapsed_ns": finished - started,
        "exit_code": exit_code, "timeout_seconds": timeout,
        "timed_out": timed_out, "launch_error": launch_error,
        "stdout": str(stdout), "stderr": str(stderr),
    }


def bind_child(output, captured):
    record = {**captured, **{f"{stream}_sha256": sha256(captured[stream]) for stream in ("stdout", "stderr")}}
    write_json(output / f"{record['label']}.json", record)
    return record


def run_child(output, label, argv, cwd, env, timeout=3600):
    return bind_child(output, capture_child(output, label, argv, cwd, env, timeout))


def passed(record, exit_code=0):
    return record["exit_code"] == exit_code and not record["timed_out"] and record["launch_error"] is None


def require_exit(record, exit_code=0):
    if not passed(record, exit_code):
        raise ValueError(f"{record['label']} did not exit {exit_code}; see {record['stderr']}")


def raw(record, stream):
    data = Path(record[stream]).read_bytes()
    if hashlib.sha256(data).hexdigest() != record[f"{stream}_sha256"]:
        raise ValueError(f"recorded {stream} changed for {record['label']}")
    return data


def unittest_outcomes(record, expected):
    text = raw(record, "stderr").decode("utf-8", errors="replace").replace("\r\n", "\n")
    observed = TEST_ROW.findall(text)
    counts = TEST_COUNT.findall(text)
    exact_order = [identity for identity, _ in observed] == list(expected)
    return {
        "expected": list(expected), "observed": observed,
        "reported_count": int(counts[-1][0]) if counts else None,
        "reported_seconds": float(counts[-1][1]) if counts else None,
        "exact_order": exact_order,
        "passed": passed(record) and exact_order and bool(counts)
        and int(counts[-1][0]) == len(expected) and all(outcome == "ok" for _, outcome in observed),
    }


def run_unittest(output, label, names, expected, oracle, env):
    record = run_child(output, label, [sys.executable, "-B", "-m", "unittest", "-v", *names], oracle, env)
    return {"process": record, **unittest_outcomes(record, expected)}


def verify_oracle(output, suffix, workload, oracle, env):
    worktree = oracle.parent.parent
    head = run_child(output, f"oracle-head-{suffix}", ["git", "rev-parse", "HEAD"], worktree, env, timeout=60)
    status = run_child(
        output, f"oracle-status-{suffix}",
        ["git", "status", "--porcelain", "--untracked-files=all", "--", "skills/software-engineering-graph"],
        worktree, env, timeout=60,
    )
    require_exit(head)
    require_exit(status)
    if raw(head, "stdout").decode("ascii").strip() != workload.head or raw(status, "stdout"):
        raise ValueError("frozen oracle revision or source tree differs")
    bound = {}
    for item in workload.modules:
        digest = sha256(worktree / item["path"])
        if digest != item["sha256"]:
            raise ValueError(f"frozen module changed: {item['path']}")
        bound[item["path"]] = digest
    return {"head": workload.head, "modules": bound, "git": [head, status]}
