"""Emit a reviewable patch for the already inspected immutable SQLite DDL data."""

import argparse
import hashlib
import sys
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--oracle", type=Path, required=True)
    args = parser.parse_args()
    source = args.oracle / "graph_engine/state.py"
    if hashlib.sha256(source.read_bytes()).hexdigest() != "18f8bfe9f525de516a49c3bab9d0430a442c9e9558ceb560d1660f287a5da983":
        raise ValueError("FROZEN_ORACLE_SOURCE_MISMATCH")
    sys.path.insert(0, str(args.oracle.resolve()))
    from graph_engine.state import SCHEMA
    target = Path(__file__).resolve().parent.parent / "src/infrastructure/state_schema.sql"
    print("*** Begin Patch")
    print("*** Add File: " + target.as_posix())
    for line in SCHEMA.splitlines():
        print("+" + line)
    print("*** End Patch")


if __name__ == "__main__":
    main()
