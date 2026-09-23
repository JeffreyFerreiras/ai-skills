#!/usr/bin/env python3
"""Manage deterministic host-only helper reservations."""

import argparse
import json
import sys
from pathlib import Path

SKILL_ROOT = Path(__file__).resolve().parents[1]
if str(SKILL_ROOT) not in sys.path:
    sys.path.insert(0, str(SKILL_ROOT))

from graph_engine.contracts import ContractError, safe_json_snapshot
from graph_engine.helper_register import HelperRegister, HelperRegisterError, MAX_RECORD_BYTES
from graph_engine.state import StateError


def _input(path: str):
    candidate = Path(path).resolve(strict=True)
    return safe_json_snapshot(candidate, [candidate.parent], MAX_RECORD_BYTES).parsed


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="helper-register")
    commands = parser.add_subparsers(dest="command", required=True)
    initialize = commands.add_parser("initialize")
    initialize.add_argument("--state-root", required=True)
    initialize.add_argument("--repo", required=True)
    initialize.add_argument("--run-id", required=True)
    initialize.add_argument("--plan", required=True)
    initialize.add_argument("--allowance", required=True)
    initialize.add_argument("--host-observation", required=True)
    for name in ("preflight", "reserve"):
        command = commands.add_parser(name)
        command.add_argument("--register", required=True)
        command.add_argument("--context", required=True)
        command.add_argument("--request", required=True)
    settle = commands.add_parser("settle")
    settle.add_argument("--register", required=True)
    settle.add_argument("--context", required=True)
    settle.add_argument("--settlement", required=True)
    status = commands.add_parser("status")
    status.add_argument("--register", required=True)
    status.add_argument("--context", required=True)
    return parser


def execute(argv=None):
    args = build_parser().parse_args(argv)
    register = HelperRegister()
    if args.command == "initialize":
        return register.initialize(
            Path(args.state_root), Path(args.repo), args.run_id, Path(args.plan),
            Path(args.allowance), Path(args.host_observation),
        )
    context = _input(args.context)
    if args.command == "preflight":
        return register.preflight(Path(args.register), context, _input(args.request))
    if args.command == "reserve":
        return register.reserve(Path(args.register), context, _input(args.request))
    if args.command == "settle":
        return register.settle(Path(args.register), context, _input(args.settlement))
    return register.status(Path(args.register), context)


def main(argv=None) -> int:
    try:
        result = execute(argv)
        exit_code = 0 if result.get("ok") else 3
    except (ContractError, HelperRegisterError, StateError) as error:
        result = {"ok": False, "code": getattr(error, "code", "INVALID_INPUT")}
        exit_code = 4
    except OSError:
        result = {"ok": False, "code": "IO_FAILURE"}
        exit_code = 5
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
