"""Inspect the actual snapshot DTO on an isolated policy fixture."""

import argparse
import json
import os
import sys
from pathlib import Path
from unittest.mock import patch


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--oracle", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    sys.path.insert(0, str(args.oracle.resolve()))
    from graph_engine import config
    args.output.mkdir(parents=True, exist_ok=False)
    repo, skill, home = (args.output / name for name in ("repo", "skill", "home"))
    repo.mkdir(); skill.mkdir()
    with patch.dict(os.environ, {"SOFTWARE_ENGINEERING_GRAPH_HOME": str(args.output / "default-home")}, clear=True):
        _, snapshot = config.load_policy(repo, skill_root=skill, runtime_root=home)
    fields = {}
    for name, value in vars(snapshot).items():
        descriptor = dict(type=type(value).__name__)
        if isinstance(value, bytes):
            descriptor["length"] = len(value)
        elif isinstance(value, (str, int, tuple, Path)):
            descriptor["value"] = str(value) if isinstance(value, Path) else value
        fields[name] = descriptor
    (args.output / "metadata.json").write_text(json.dumps(fields, indent=2)+"\n", encoding="ascii")
    print(json.dumps(fields))


if __name__ == "__main__":
    main()
