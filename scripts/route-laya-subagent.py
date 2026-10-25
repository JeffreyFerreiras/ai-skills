"""Compatibility entry point for the installed Laya subagent instructions."""

from pathlib import Path
import runpy

if __name__ == "__main__":
    client = runpy.run_path(str(Path(__file__).with_name("laya-hook.py")))
    raise SystemExit(client["main"](subagent=True))
