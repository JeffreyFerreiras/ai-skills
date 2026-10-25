"""Install lightweight Codex clients for the existing resident LayaCodex service."""

import argparse
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile

SOURCE = Path(__file__).resolve().parent
REPOSITORY = SOURCE.parents[1]
SCRIPTS = REPOSITORY / "scripts"
START = "<!-- laya-subagents:start -->"
END = "<!-- laya-subagents:end -->"


def write_text(path, value):
    with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=path.parent,
                                     prefix=path.name + ".", suffix=".tmp", delete=False) as output:
        output.write(value)
        temporary = Path(output.name)
    try:
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def read_json(path):
    result = json.loads(path.read_text(encoding="utf-8-sig")) if path.exists() else {}
    if not isinstance(result, dict):
        raise ValueError(f"Expected a JSON object in {path.name}")
    return result


def subagent_instructions(existing, runtime, python):
    section = (SOURCE / "subagent-instructions.md").read_text(encoding="utf-8")
    section = section.replace("__LAYA_PYTHON__", python.as_posix().replace("'", "''"))
    section = section.replace("__LAYA_ROUTER__", (runtime / "route_subagent.py").as_posix().replace("'", "''"))
    if START not in existing and END not in existing:
        return existing + ("\n\n" if existing else "") + section
    if existing.count(START) != 1 or existing.count(END) != 1 or existing.index(END) < existing.index(START):
        raise ValueError("Malformed Laya instruction markers; repair the existing file first.")
    return existing[:existing.index(START)] + section.rstrip("\n") + existing[existing.index(END) + len(END):]


def owned_handler(handler, runtime, service_root):
    scripts = {str(path).replace("\\", "/").casefold() for path in (
        runtime / "hook.py", runtime / "laya-hook.py", service_root / "laya-hook.py",
    )}
    startup = (runtime / "start-hook.ps1").as_posix().casefold()
    for field in ("command", "commandWindows", "command_windows"):
        try:
            args = [part.strip('"').replace("\\", "/").casefold()
                    for part in shlex.split(handler.get(field, ""), posix=False)]
        except ValueError:
            continue
        if len(args) == 5 and args[1:] == ["-noprofile", "-noninteractive", "-file", startup]:
            return True
        if len(args) == 3 and args[1] == "-b":
            args.pop(1)
        if len(args) == 2 and args[1] in scripts and Path(args[0]).name in {
            "python", "python.exe", "python3", "python3.exe", "pythonw.exe",
        }:
            return True
    return False


def merge_hooks(document, runtime, service_root, registration):
    hooks = document.setdefault("hooks", {})
    if not isinstance(hooks, dict):
        raise ValueError("Expected a hooks object")
    for event in ("UserPromptSubmit", "SessionStart"):
        if event not in hooks:
            continue
        kept = []
        for group in hooks[event]:
            remaining = [handler for handler in group["hooks"]
                         if not owned_handler(handler, runtime, service_root)]
            if remaining or not group["hooks"]:
                kept.append({**group, "hooks": remaining})
        if kept:
            hooks[event] = kept
        else:
            hooks.pop(event)
    if registration:
        hooks.setdefault("UserPromptSubmit", []).append({"hooks": [registration]})
    return document


def parse_args(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--codex-home", type=Path,
                        default=Path(os.environ.get("CODEX_HOME", Path.home() / ".codex")))
    parser.add_argument("--python", type=Path, default=Path(sys.executable))
    parser.add_argument("--service-root", type=Path, default=Path("C:/models/laya"))
    parser.add_argument("--service-port", type=int, default=8766)
    parser.add_argument("--integration", choices=("subagents", "hooks", "both"), default="subagents")
    parser.add_argument("--dry-run", action="store_true")
    return parser.parse_args(argv)


def install(args):
    if os.name != "nt":
        raise ValueError("The profile installer currently supports Windows only.")
    home = args.codex_home.expanduser().resolve()
    runtime = (home / "laya-hook").resolve()
    python = args.python.expanduser().resolve()
    service_root = args.service_root.expanduser().resolve()
    if runtime.is_relative_to(REPOSITORY) or service_root.is_relative_to(REPOSITORY):
        raise ValueError("Runtime storage must stay outside the source repository.")
    if not python.is_file():
        raise ValueError("Supply an existing Python executable with --python.")
    if not 1 <= args.service_port <= 65535:
        raise ValueError("Port must be between 1 and 65535.")
    config_path, hooks_path = runtime / "config.json", home / "hooks.json"
    config = read_json(config_path)
    config.update(service_port=args.service_port, enabled=args.integration in ("hooks", "both"),
                  subagent_routing_enabled=args.integration in ("subagents", "both"))
    config.setdefault("mode", "advisory")
    if config["mode"] not in ("advisory", "observe"):
        raise ValueError("Existing mode must be advisory or observe.")
    original_hooks = hooks_path.read_bytes() if hooks_path.exists() else None
    instructions_path = home / "AGENTS.override.md"
    if not instructions_path.exists() or not instructions_path.read_text(encoding="utf-8-sig").strip():
        instructions_path = home / "AGENTS.md"
    original_instructions = instructions_path.read_bytes() if instructions_path.exists() else None
    instructions = None
    if config["subagent_routing_enabled"]:
        instructions = subagent_instructions(
            original_instructions.decode("utf-8-sig") if original_instructions else "", runtime, python)
    registration = None
    if config["enabled"]:
        registration = {"type": "command", "command": subprocess.list2cmdline([
            python.as_posix(), "-B", (runtime / "laya-hook.py").as_posix(),
        ]), "timeout": 5, "additionalContextLimit": 256,
            "statusMessage": "Checking local Laya task advice"}
    hooks = merge_hooks(read_json(hooks_path), runtime, service_root, registration)
    files = {"laya-hook.py": "laya-hook.py", "laya_routing.py": "laya_routing.py",
             "route-laya-subagent.py": "route_subagent.py"}
    if not all((SCRIPTS / name).is_file() for name in files):
        raise ValueError("Missing client source files.")
    print(f"Client runtime: {runtime}\nResident service port: {args.service_port}\nIntegration: {args.integration}")
    if args.dry_run:
        print("Dry run: no files, packages, services, processes, or hook trust changed.")
        return
    runtime.mkdir(parents=True, exist_ok=True)
    for source, target in files.items():
        shutil.copy2(SCRIPTS / source, runtime / target)
    for path, original in ((hooks_path, original_hooks), (instructions_path, original_instructions)):
        if (path.read_bytes() if path.exists() else None) != original:
            raise ValueError("Profile changed during installation; rerun to merge the latest files.")
    write_text(config_path, json.dumps(config, indent=2) + "\n")
    if original_hooks is not None or registration:
        write_text(hooks_path, json.dumps(hooks, indent=2) + "\n")
    if instructions is not None:
        write_text(instructions_path, instructions)
    print("Installed clients. Retained legacy files and data; no service was started or stopped.")
    print("Review changed hook definitions in /hooks. New chats load updated instructions.")


if __name__ == "__main__":
    try:
        install(parse_args())
    except (ValueError, OSError, TypeError, KeyError) as error:
        print(f"Installation failed: {error}", file=sys.stderr)
        sys.exit(1)
