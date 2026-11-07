"""Freeze a bounded, parser-only corpus from the unchanged graph CLI."""

import argparse
import contextlib
import hashlib
import importlib
import io
import json
import os
from pathlib import Path
import sys


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def typed(value):
    if value is None:
        return {"kind": "NoneType", "value": None}
    if type(value) is bool:
        return {"kind": "bool", "value": value}
    if type(value) is int:
        return {"kind": "int", "value": str(value)}
    if type(value) is str:
        return {"kind": "str", "value": value}
    raise TypeError(f"Unexpected namespace value type: {type(value).__name__}")


def actions(parser):
    return [a for a in parser._actions if a.option_strings and a.dest != "help"]


def sample(action):
    if action.choices is not None:
        return str(next(iter(action.choices)))
    return "1" if action.type is int else "value"


def option(action, value=None):
    name = action.option_strings[-1]
    return [name] if action.nargs == 0 else [name, sample(action) if value is None else value]


def enumerate_parsers(root):
    nodes = []
    leaves = []

    def walk(parser, path):
        nodes.append((path, parser))
        selectors = [a for a in parser._actions if isinstance(a, argparse._SubParsersAction)]
        if not selectors:
            leaves.append((path, parser))
        for selector in selectors:
            for name, child in selector.choices.items():
                walk(child, [*path, name])

    walk(root, [])
    assert len(leaves) == 34, len(leaves)
    return nodes, leaves


def corpus(root, nodes, leaves):
    cases = []

    def add(family, argv, columns="80"):
        cases.append({"id": len(cases), "family": family, "argv": argv, "columns": columns})

    root_actions = actions(root)
    integer_actions = []
    for path, parser in nodes:
        for width in ("20", "40", "80", "120"):
            for help_option in ("--help", "-h"):
                add("help", [*path, help_option], width)
        add("missing-command-or-required", path)
        if parser is not root and path:
            add("unknown-before-selector", ["--unknown-root", *path])

    for path, parser in leaves:
        declared = actions(parser)
        required = [a for a in declared if a.required]

        def minimal(omit=None):
            return [*path, *(arg for a in required if a is not omit for arg in option(a))]

        base = minimal()
        add("valid-minimal-defaults", base)
        add("unknown-after-valid", [*base, "--unknown-child"])
        add("unknown-root-and-child", ["--unknown-root", *base, "--unknown-child"])
        add("terminator", [*base, "--"])
        for action in required:
            add("missing-required", minimal(action))
        for action in root_actions:
            add("root-before-selector", [*option(action), *base])
            add("root-after-selector", [*base, *option(action)])
        for action_index, action in enumerate(declared):
            if not action.required:
                add("optional-or-explicit", [*base, *option(action)])
            add("duplicate-option", [*base, *option(action), *option(action)])
            name = action.option_strings[-1]
            add("explicit-equals", [*base, name + "=" + sample(action)])
            if action_index == 0 and len(name) > 3:
                add("abbreviation", [*base, *([name[:-1]] if action.nargs == 0 else [name[:-1], sample(action)])])
            if action.choices is not None:
                add("invalid-choice-before-help", [*minimal(action), name, "bad-choice", "--help"])
            if action.type is int:
                integer_actions.append((path, action.dest))
                for value in ("0", "-1", "+2", str(1 << 180), "9" * 4301, "١_٢", "１２", "1_000", "1__0", "_1", "1_", "1.0", "nan", "", " ", "\u200312\u2003", "-١", "\ud800"):
                    add("integer", [*minimal(action), name, value])
            if action is next((a for a in declared if a.nargs != 0 and a.type is not int), None):
                for value in ("ascii", "雪\u2003é", "\ud800", "--", "-1", ""):
                    add("string-value", [*minimal(action), name + "=" + value])
        for tail in ("--s", "--a", "--r", "--help=bad", "-hx", "-h-bad", "-hi there", "\ud800", "雪"):
            add("ambiguous-help-tail-unicode", [*base, tail])
    assert len(integer_actions) == 4, integer_actions
    for columns in ("0", "-1", "abc", "", "١٢٠", "1_20", "+40", " 80 "):
        add("columns-edge", ["--help"], columns)
    for argv in (["--state", "policy"], ["--help", "--state"], ["record", "usage", "--help", "--s"], ["--repo", "--", "policy"], ["--", "policy"], ["policy", "--", "--help"]):
        add("ordering", argv)
    assert len(cases) <= 2048, len(cases)
    return cases


def main():
    cli = argparse.ArgumentParser()
    cli.add_argument("--oracle", type=Path, required=True)
    cli.add_argument("--output", type=Path, required=True)
    args = cli.parse_args()
    if args.output.exists():
        raise FileExistsError("The one-shot parser corpus already has an output directory")
    sys.path.insert(0, str(args.oracle))
    module = importlib.import_module("graph_engine.cli")
    root = module.build_parser()
    nodes, leaves = enumerate_parsers(root)
    cases = corpus(root, nodes, leaves)
    host = next(a for path, parser in leaves if path == ["plan"] for a in actions(parser) if a.dest == "host")
    phase = next(a for path, parser in leaves if path == ["record", "usage"] for a in actions(parser) if a.dest == "phase")
    source_paths = sorted((args.oracle / "graph_engine").glob("*.py"))
    provenance = {
        "python_executable": sys.executable,
        "python_executable_sha256": digest(sys.executable),
        "python_version": sys.version,
        "generator_sha256": digest(__file__),
        "argparse_sha256": digest(argparse.__file__),
        "source_sha256": {str(path.relative_to(args.oracle)): digest(path) for path in source_paths},
        "max_parse_args_calls": 2048,
        "planned_parse_args_calls": len(cases),
        "leaf_paths": [path for path, _ in leaves],
        "integer_digit_limit": sys.get_int_max_str_digits(),
        "recursion_limit": sys.getrecursionlimit(),
        "environment": {key: os.environ.get(key) for key in ("COLUMNS", "LINES", "PYTHONIOENCODING", "PYTHONUTF8")},
        "graph_spec_inputs": {"hosts": list(host.choices), "default_host": host.default, "usage_phases": list(phase.choices)},
        "scope": "build_parser and parse_args only; no handler, state, main, execute, subprocess or validation execution",
    }
    args.output.mkdir(parents=True)
    (args.output / "planned.json").write_text(json.dumps({"provenance": provenance, "cases": cases}, ensure_ascii=True, indent=2) + "\n", encoding="utf-8")
    records = []
    previous_columns = os.environ.get("COLUMNS")
    calls = 0
    try:
        for case in cases:
            os.environ["COLUMNS"] = case["columns"]
            stdout, stderr = io.StringIO(), io.StringIO()
            record = {**case, "exit_code": 0, "arguments": None, "namespace_order": None, "exception": None}
            with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                try:
                    calls += 1
                    namespace = root.parse_args(case["argv"])
                    record["arguments"] = {name: typed(value) for name, value in vars(namespace).items()}
                    record["namespace_order"] = list(vars(namespace))
                except SystemExit as error:
                    record["exit_code"] = error.code
                    record["exception"] = {"kind": "SystemExit", "code": typed(error.code)}
                except Exception as error:
                    record["exit_code"] = None
                    record["exception"] = {"kind": type(error).__name__, "message": str(error)}
            record["stdout"], record["stderr"] = stdout.getvalue(), stderr.getvalue()
            records.append(record)
    finally:
        if previous_columns is None:
            os.environ.pop("COLUMNS", None)
        else:
            os.environ["COLUMNS"] = previous_columns
    assert calls == len(cases) <= 2048
    source_after = {str(path.relative_to(args.oracle)): digest(path) for path in source_paths}
    assert source_after == provenance["source_sha256"], "Oracle source changed during capture"
    assert digest(__file__) == provenance["generator_sha256"], "Generator changed during capture"
    assert digest(sys.executable) == provenance["python_executable_sha256"], "Python executable changed during capture"
    assert digest(argparse.__file__) == provenance["argparse_sha256"], "argparse changed during capture"
    result = {"provenance": provenance, "actual_parse_args_calls": calls, "source_guard_after": source_after, "records": records}
    (args.output / "report.json").write_text(json.dumps(result, ensure_ascii=True, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"parse_args_calls": calls, "leaves": len(leaves), "records": len(records)}))


if __name__ == "__main__":
    main()
