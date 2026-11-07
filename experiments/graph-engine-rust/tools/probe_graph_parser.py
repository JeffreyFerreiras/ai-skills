"""Freeze bounded observations of the original graph parser, without handlers."""

import argparse
import contextlib
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import sys
import textwrap


MAX_CALLS = 2048


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def command_leaves(parser, prefix=()):
    subcommands = [action for action in parser._actions if isinstance(action, argparse._SubParsersAction)]
    if not subcommands:
        yield prefix, parser
    else:
        assert len(subcommands) == 1
        for name, child in subcommands[0].choices.items():
            yield from command_leaves(child, (*prefix, name))


def value_for(action):
    if action.choices is not None:
        return str(next(iter(action.choices)))
    return "3" if action.type is int else "fixture"


def corpus(parser):
    leaves = list(command_leaves(parser))
    assert len(leaves) == 34, ("ORIGINAL_LEAF_COUNT_CHANGED", len(leaves))
    cases = []
    seen = set()

    def add(label, argv, columns="80"):
        key = (columns, tuple(argv))
        if key not in seen:
            seen.add(key)
            cases.append(dict(label=label, argv=list(argv), columns=columns))

    for columns in ("20", "40", "80", "120", "١_٢", "4_0", "0", "-1"):
        for option in ("--help", "--he", "-h", "-hmore", "-h-bad", "--help=value"):
            add("root-help", [option], columns)
    for argv in ([], ["--"], ["--", "--help"], ["unknown"], ["Σ"], ["\ud800"],
                 ["--repo"], ["--state-root"], ["--ack"], ["--unknown", "--help"],
                 ["--unknown", "status"], ["-hi there"], ["--repo", "--", "status"]):
        add("root-error", argv)
    integer_values = ("0", "-1", "-1\n", "١_٢", "+ 1", "-0.0", "1e2", "1_", "1__0", "  +１２  ", "NaN", "9" * 180, "9" * 4301)
    for path, leaf in leaves:
        prefix = list(path)
        required = [action for action in leaf._actions if action.required and action.option_strings]
        minimal = prefix + [part for action in required for part in (action.option_strings[0], value_for(action))]
        add("minimal", minimal)
        for missing in required:
            add("missing-required", prefix + [part for action in required if action is not missing for part in (action.option_strings[0], value_for(action))])
        for columns in ("20", "40", "80", "120"):
            add("leaf-help", prefix + ["--help"], columns)
        for suffix in (["--unknown"], ["--unknown", "--help"], ["--s"], ["--repo", "fixture"], ["--", "--help"], ["--run-id=\ud800"], ["extra", "\udfff"]):
            add("leaf-order", minimal + suffix)
        for root in (["--repo", "fixture"], ["--state-root", "fixture"], ["--ack-degraded-permissions"], ["--ack-degraded-durability"]):
            add("root-before", root + minimal)
        all_options = list(minimal)
        for action in leaf._actions:
            if not action.option_strings or isinstance(action, argparse._HelpAction):
                continue
            option = action.option_strings[0]
            if not action.required:
                all_options.append(option)
                if action.nargs != 0:
                    all_options.append(value_for(action))
            if action.required:
                add("duplicate", minimal + [option, value_for(action)])
            if action.nargs == 0:
                add("explicit-flag", minimal + [option + "=true"])
            if action.choices is not None:
                add("invalid-choice", minimal + [option, "not-a-choice"])
            if action.type is int:
                for value in integer_values:
                    add("integer", minimal + [option, value])
        add("all-options", all_options)
    assert len(cases) <= MAX_CALLS, ("REFERENCE_CALL_BOUND_EXCEEDED", len(cases))
    return cases


def main():
    arguments = argparse.ArgumentParser()
    arguments.add_argument("--oracle-root", type=Path, required=True)
    arguments.add_argument("--output", type=Path, required=True)
    args = arguments.parse_args()
    artifact = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve(strict=True)
    output = args.output.resolve()
    if not output.is_relative_to(artifact) or output == artifact:
        raise ValueError("OUTPUT_OUTSIDE_OWNED_ART")
    original = args.oracle_root.resolve(strict=True)
    cli_path = original / "graph_engine" / "cli.py"
    before = digest(cli_path)
    generator = digest(Path(__file__))
    sys.path.insert(0, str(original))
    from graph_engine.cli import build_parser

    parser = build_parser()
    cases = corpus(parser)
    previous = os.environ.get("COLUMNS")
    records = []
    active = False

    def audit(event, event_arguments):
        if active and (event == "open" or event.startswith(("subprocess.", "socket.", "sqlite3.")) or event in {"os.system", "os.chdir", "os.remove", "os.rename", "os.mkdir"}):
            raise RuntimeError("ORACLE_PROBE_FORBIDDEN_EVENT:" + event)

    sys.addaudithook(audit)
    try:
        for case in cases:
            os.environ["COLUMNS"] = case["columns"]
            stdout, stderr = io.StringIO(), io.StringIO()
            result, exit_code, exception = None, 0, None
            try:
                active = True
                with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                    result = vars(parser.parse_args(case["argv"]))
            except SystemExit as error:
                exit_code = error.code
            except Exception as error:
                if str(error).startswith("ORACLE_PROBE_FORBIDDEN_EVENT:"):
                    raise
                exception = dict(kind=type(error).__name__, message=str(error))
            finally:
                active = False
            records.append(dict(**case, arguments=result, exit_code=exit_code, exception=exception, stdout=stdout.getvalue(), stderr=stderr.getvalue()))
    finally:
        active = False
        if previous is None:
            os.environ.pop("COLUMNS", None)
        else:
            os.environ["COLUMNS"] = previous
    if digest(cli_path) != before or digest(Path(__file__)) != generator:
        raise ValueError("SOURCE_CHANGED_DURING_REFERENCE")
    output.mkdir(parents=True, exist_ok=False)
    report = dict(count=len(records), maximum_calls=MAX_CALLS, records=records,
                  oracle=dict(executable=sys.executable, executable_sha256=digest(Path(sys.executable)), version=sys.version,
                              cli_sha256=before), generator_sha256=generator,
                  scope="Parser only; no handlers, data I/O, processes, state, profiles or session access")
    raw = (json.dumps(report, ensure_ascii=True, indent=2) + "\n").encode("ascii")
    (output / "report.json").write_bytes(raw)
    print(json.dumps(dict(report=str(output / "report.json"), count=len(records), sha256=hashlib.sha256(raw).hexdigest())))


if __name__ == "__main__":
    main()
