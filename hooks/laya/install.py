"""Install the Windows Codex hook without copying local runtime data into Git."""

import argparse
import json
import os
import secrets
import shlex
import shutil
import subprocess
import sys
import tempfile
import venv
from pathlib import Path

SOURCE = Path(__file__).resolve().parent
REPOSITORY = SOURCE.parents[1]
RUNTIME_FILES = ("hook.py", "server.py", "manage.ps1", "start-hook.ps1", "README.md", "route_subagent.py")


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8-sig")) if path.exists() else {}


def write_text(path, value):
    with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=path.parent,
                                     prefix=path.name + ".", suffix=".tmp", delete=False) as output:
        output.write(value)
        temporary = Path(output.name)
    try:
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def write_json(path, value):
    write_text(path, json.dumps(value, indent=2) + "\n")


def subagent_instructions(existing, runtime):
    start, end = "<!-- laya-subagents:start -->", "<!-- laya-subagents:end -->"
    section = (SOURCE / "subagent-instructions.md").read_text(encoding="utf-8")
    section = section.replace("__LAYA_PYTHON__", (runtime / ".venv/Scripts/python.exe").as_posix().replace("'", "''"))
    section = section.replace("__LAYA_ROUTER__", (runtime / "route_subagent.py").as_posix().replace("'", "''"))
    if start not in existing and end not in existing:
        return existing + ("\n\n" if existing else "") + section
    if existing.count(start) != 1 or existing.count(end) != 1 or existing.index(end) < existing.index(start):
        raise ValueError("Malformed Laya instruction markers; preserve and repair the existing file first.")
    return existing[:existing.index(start)] + section.rstrip("\n") + existing[existing.index(end) + len(end):]


def hook_registrations(runtime, powershell):
    python = runtime / ".venv" / "Scripts" / "python.exe"
    prompt_command = subprocess.list2cmdline([python.as_posix(), (runtime / "hook.py").as_posix()])
    startup_command = subprocess.list2cmdline([
        powershell.as_posix(), "-NoProfile", "-NonInteractive", "-File",
        (runtime / "start-hook.ps1").as_posix(),
    ])
    return {
        "UserPromptSubmit": {"hooks": [{
            "type": "command", "command": prompt_command, "async": False,
            "timeout": 4, "additionalContextLimit": 256, "statusMessage": "Laya routing",
        }]},
        "SessionStart": {"matcher": "startup|resume|clear|fork", "hooks": [{
            "type": "command", "command": startup_command, "async": True, "timeout": 10,
        }]},
    }


def merge_hooks(document, registrations, runtime):
    hooks = document.setdefault("hooks", {})
    python = (runtime / ".venv" / "Scripts" / "python.exe").as_posix().lower()
    prompt_script = (runtime / "hook.py").as_posix().lower()
    startup_script = (runtime / "start-hook.ps1").as_posix().lower()
    for event, registration in registrations.items():
        kept = []
        for group in hooks.get(event, []):
            remaining = []
            for handler in group["hooks"]:
                try:
                    arguments = [argument.strip('"').replace("\\", "/").lower()
                                 for argument in shlex.split(handler.get("command", ""), posix=False)]
                except ValueError:
                    arguments = []
                owned = arguments == [python, prompt_script] or (
                    len(arguments) == 5 and arguments[1:] == [
                        "-noprofile", "-noninteractive", "-file", startup_script,
                    ]
                )
                if not owned:
                    remaining.append(handler)
            if remaining:
                kept.append({**group, "hooks": remaining})
        hooks[event] = [*kept, registration]
    return document


def parse_args(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model-path", type=Path, required=True,
                        help="Existing local English Laya checkpoint; weights are never copied or downloaded.")
    parser.add_argument("--codex-home", type=Path,
                        default=Path(os.environ.get("CODEX_HOME", Path.home() / ".codex")))
    parser.add_argument("--powershell", type=Path)
    parser.add_argument("--integration", choices=("hooks", "subagents"), default="hooks",
                        help="Install prompt hooks or active subagent routing instructions for normal chats.")
    parser.add_argument("--mode", choices=("advisory", "observe"))
    parser.add_argument("--device", choices=("auto", "cpu", "cuda"))
    parser.add_argument("--port", type=int)
    parser.add_argument("--reuse-system-packages", action="store_true")
    parser.add_argument("--skip-dependencies", action="store_true",
                        help="Reuse an existing .venv without changing its packages.")
    parser.add_argument("--dry-run", action="store_true")
    return parser.parse_args(argv)


def install(args):
    if os.name != "nt":
        raise ValueError("This package currently supports Windows and PowerShell 7 only.")
    codex_home = args.codex_home.expanduser().resolve()
    runtime = (codex_home / "laya-hook").resolve()
    model_path = args.model_path.expanduser().resolve()
    if runtime.is_relative_to(REPOSITORY) or model_path.is_relative_to(REPOSITORY):
        raise ValueError("Runtime and model storage must stay outside the source repository.")
    required = ("model.safetensors", "rl_agent_config.json", "encoder/config.json", "tokenizer/tokenizer.json")
    if not all((model_path / name).is_file() for name in required):
        raise ValueError("Model path must contain a complete downloaded English Laya checkpoint.")
    powershell = args.powershell or Path(shutil.which("pwsh") or "")
    if not powershell.is_file():
        raise ValueError("PowerShell 7 is required; put pwsh on PATH or supply --powershell.")
    powershell = powershell.resolve()
    config_path = runtime / "config.json"
    hooks_path = codex_home / "hooks.json"
    config = read_json(config_path)
    config.update({
        "enabled": config.get("enabled", args.integration == "hooks"),
        "model_path": str(model_path),
        "mode": args.mode or config.get("mode", "advisory"),
        "device": args.device or config.get("device", "auto"),
        "port": args.port if args.port is not None else config.get("port", 18147),
    })
    if type(config["port"]) is not int or not 1 <= config["port"] <= 65535:
        raise ValueError("Port must be between 1 and 65535.")
    if config["mode"] not in {"advisory", "observe"} or config["device"] not in {"auto", "cpu", "cuda"}:
        raise ValueError("Existing mode or device is invalid; pass an explicit supported value.")
    original_hooks = hooks_path.read_bytes() if hooks_path.exists() else None
    instructions_path = codex_home / "AGENTS.override.md"
    if not instructions_path.exists() or not instructions_path.read_text(encoding="utf-8-sig").strip():
        instructions_path = codex_home / "AGENTS.md"
    original_instructions = instructions_path.read_bytes() if instructions_path.exists() else None
    if args.integration == "subagents":
        config.update({"subagent_routing_enabled": True, "powershell": str(powershell)})
        instructions = subagent_instructions(
            original_instructions.decode("utf-8-sig") if original_instructions else "", runtime)
    else:
        hooks = merge_hooks(read_json(hooks_path), hook_registrations(runtime, powershell), runtime)
    print(f"Runtime: {runtime}\nModel: {model_path}\nIntegration: {args.integration}")
    if args.dry_run:
        print("Dry run: no files, environments, packages, or hook trust changed.")
        return
    python = runtime / ".venv" / "Scripts" / "python.exe"
    if args.skip_dependencies and not python.is_file():
        raise ValueError("--skip-dependencies requires an existing runtime .venv.")
    runtime.mkdir(parents=True, exist_ok=True)
    if not args.skip_dependencies:
        if not python.is_file():
            venv.EnvBuilder(with_pip=True, system_site_packages=args.reuse_system_packages).create(runtime / ".venv")
        subprocess.run([str(python), "-m", "pip", "install", "--disable-pip-version-check",
                        "--timeout", "30", "-r", str(SOURCE / "requirements.txt")], check=True, timeout=600)
    for name in RUNTIME_FILES:
        shutil.copy2(SOURCE / name, runtime / name)
    write_json(config_path, config)
    token_path = runtime / "token.txt"
    if not token_path.exists():
        with token_path.open("x", encoding="utf-8") as token_file:
            token_file.write(secrets.token_urlsafe(32))
    if args.integration == "subagents":
        if (instructions_path.read_bytes() if instructions_path.exists() else None) != original_instructions:
            raise ValueError("Agent instructions changed during installation; rerun to merge the latest file.")
        write_text(instructions_path, instructions)
        print(f"Installed subagent routing in {instructions_path}. New chats load it; existing chats must read it.")
    else:
        if (hooks_path.read_bytes() if hooks_path.exists() else None) != original_hooks:
            raise ValueError("Hook configuration changed during installation; rerun to merge the latest file.")
        write_json(hooks_path, hooks)
        print("Installed. Review new or changed definitions in /hooks; hook trust was not modified.")


if __name__ == "__main__":
    try:
        install(parse_args())
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        print(f"Installation failed: {error}", file=sys.stderr)
        sys.exit(1)
