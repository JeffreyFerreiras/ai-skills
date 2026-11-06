"""Capture bounded parser-only contracts; never execute a helper operation."""

import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import sys


FIELDS = {
    "initialize": ("state-root", "repo", "run-id", "plan", "allowance", "host-observation"),
    "preflight": ("register", "context", "request"),
    "reserve": ("register", "context", "request"),
    "settle": ("register", "context", "settlement"),
    "status": ("register", "context"),
}


def vectors():
    rows = [[], ["-h"], ["--help"], ["init"], ["unknown"], ["--unknown"], ["--help", "unknown"], ["-h=value"], ["-help"]]
    for command, fields in FIELDS.items():
        valid = [command] + [part for field in fields for part in ("--" + field, field + "-value")]
        rows.extend(([command], [command, "-h"], [command, "--help"], valid, valid + ["extra"], valid + ["--unknown"], valid + ["--"], valid + ["--", "extra"], ["--", command], [command, "--help", "--unknown"], [command, "--unknown", "--help"]))
        for field in fields:
            rows.extend(([command, "--" + field], valid + ["--" + field, "replacement"], [command, "--" + field + "=value"], [command, "--" + field, "--unknown"]))
        for prefix in ("--r", "--re", "--sta", "--c", "--h", "--p", "--s", "--a"):
            rows.append(valid + [prefix, "prefix-value"])
        for value in ("", "-1", "-1.2", "-.5", "-1e2", "-x", "--", "--help", "None", "C:\\fixture\\path", "\u03a3", "\ud800"):
            rows.append([command, "--" + fields[0], value] + valid[3:])
        rows.extend((valid + ["-h=value"], valid + ["--help=value"], valid + ["--" + fields[0] + "=a=b"], [command, "extra"] + valid[1:]))
    return list(dict.fromkeys(tuple(row) for row in rows))


def capture(output):
    artifact_root = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve()
    output.resolve().relative_to(artifact_root)
    script = Path(__file__).resolve().parents[3] / "skills/software-engineering-graph/scripts/helper-register.py"
    spec = importlib.util.spec_from_file_location("helper_parser_oracle", script)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    records = []
    for argv in vectors():
        stdout, stderr = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            try:
                namespace = module.build_parser().parse_args(list(argv))
                result = {"exit_code": 0, "arguments": vars(namespace)}
            except SystemExit as error:
                result = {"exit_code": error.code, "arguments": None}
        records.append({"argv": list(argv), **result, "stdout": stdout.getvalue(), "stderr": stderr.getvalue()})
    report = {"scope": "parser only; build_parser().parse_args; execute/main never called", "oracle": {"executable": sys.executable, "version": sys.version, "script": str(script), "script_sha256": hashlib.sha256(script.read_bytes()).hexdigest()}, "environment": {"COLUMNS": os.environ.get("COLUMNS")}, "count": len(records), "records": records}
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes((json.dumps(report, sort_keys=True, ensure_ascii=True, indent=2) + "\n").encode("ascii"))
    return {"output": str(output), "count": len(records), "sha256": hashlib.sha256(output.read_bytes()).hexdigest()}


def capture_edges(output):
    artifact_root = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve()
    output.resolve().relative_to(artifact_root)
    script = Path(__file__).resolve().parents[3] / "skills/software-engineering-graph/scripts/helper-register.py"
    spec = importlib.util.spec_from_file_location("helper_parser_edge_oracle", script)
    module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
    flags = ("--he", "--hel", "--h=value", "--he=value", "--hel=value", "-hfoo", "-help", "-hh", "-hx", "-h=value", "--help=value", "--help")
    inputs = [("80", [flag]) for flag in flags]
    for command, fields in FIELDS.items():
        valid = [command] + [part for field in fields for part in ("--" + field, field + "-value")]
        inputs += [("80", [command, flag]) for flag in flags]
        inputs += [("80", [command, "--" + fields[0], value] + valid[3:]) for value in ("-1\n", "-1\r\n", "-١\n", "-0.5\n", "-.5\n", "-1\n\n")]
        inputs += [("80", valid + tail) for tail in (["first", "--unknown", "second"], ["--unknown", "first", "second"])]
    for columns in ("4_0", "٤٠", "+40", " 40 "):
        inputs += [(columns, ["--help"]), *[(columns, [command, "--help"]) for command in FIELDS]]
    if len(inputs) > 144:
        raise ValueError("EDGE_CORPUS_SCOPE_EXCEEDED")
    previous = os.environ.get("COLUMNS"); records = []
    try:
        for columns, argv in inputs:
            os.environ["COLUMNS"] = columns
            stdout, stderr = io.StringIO(), io.StringIO()
            with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                try:
                    namespace = module.build_parser().parse_args(argv)
                    result = {"exit_code": 0, "arguments": vars(namespace)}
                except SystemExit as error:
                    result = {"exit_code": error.code, "arguments": None}
            records.append({"columns": columns, "argv": argv, **result, "stdout": stdout.getvalue(), "stderr": stderr.getvalue()})
    finally:
        if previous is None: os.environ.pop("COLUMNS", None)
        else: os.environ["COLUMNS"] = previous
    report = {"scope": "parser only edge cases; noexecute/main/state", "environment": {"COLUMNS": "80"}, "oracle": {"executable": sys.executable, "version": sys.version, "script_sha256": hashlib.sha256(script.read_bytes()).hexdigest()}, "records": records, "count": len(records), "generator_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes((json.dumps(report, ensure_ascii=True, sort_keys=True, indent=2) + "\n").encode("ascii"))
    return {"output": str(output), "count": len(records), "sha256": hashlib.sha256(output.read_bytes()).hexdigest()}


def main():
    if len(sys.argv) == 3 and sys.argv[2] == "--edge-cases":
        print(json.dumps(capture_edges(Path(sys.argv[1])), sort_keys=True))
    elif len(sys.argv) == 3 and sys.argv[2] == "--additional-widths":
        records = []
        for width in (20, 40, 120):
            os.environ["COLUMNS"] = str(width)
            records.append(capture(Path(sys.argv[1]) / (str(width) + ".json")))
        print(json.dumps(records, sort_keys=True))
    else:
        print(json.dumps(capture(Path(sys.argv[1])), sort_keys=True))


if __name__ == "__main__":
    main()
