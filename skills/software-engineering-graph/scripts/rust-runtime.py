#!/usr/bin/env python3
"""Build the bundled native graph once in a writable cache, then execute it."""

import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys


COMMANDS = ("graphctl", "helper-register")
PACKAGE = Path("experiments/graph-engine-rust")


class RuntimeError(Exception):
    pass


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def cache_root(skill):
    override = os.environ.get("GRAPH_RUST_CACHE_HOME")
    if override is not None:
        root = Path(override)
    else:
        base = Path(os.environ.get("XDG_CACHE_HOME", str(Path.home() / ".cache")))
        root = base / "software-engineering-graph"
    if not root.is_absolute():
        raise RuntimeError("Rust cache path must be absolute")
    root = root.resolve()
    if root == skill or skill in root.parents:
        raise RuntimeError("Rust cache must be outside the immutable skill directory")
    return root


def source_key(skill):
    source = skill / "rust-source"
    if not (source / PACKAGE / "Cargo.toml").is_file():
        raise RuntimeError("Bundled Rust source is missing; reinstall the source-only Rust skill")
    digest = hashlib.sha256()
    digest.update(f"graph-rust-runtime-v1:{sys.platform}:{platform.machine()}\0".encode())
    files = sorted(source.rglob("*")) + [skill / "scripts/rust-runtime.py"]
    for path in files:
        if path.is_symlink():
            raise RuntimeError("Bundled Rust source must not contain symbolic links")
        if path.is_file():
            digest.update(path.relative_to(skill).as_posix().encode() + b"\0")
            digest.update(bytes.fromhex(sha256(path)))
    return digest.hexdigest()


def binaries(entry):
    suffix = ".exe" if os.name == "nt" else ""
    return {name: entry / "target/release" / (name + suffix) for name in COMMANDS}


def verified_cache(entry, key):
    receipt_path = entry / "complete.json"
    if not receipt_path.exists():
        return False
    try:
        receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
        expected = receipt["binaries"]
        valid = receipt["source_key"] == key and set(expected) == set(COMMANDS)
        for name, binary in binaries(entry).items():
            valid = valid and binary.is_file() and not binary.is_symlink() and sha256(binary) == expected[name]
        if not valid:
            raise ValueError("receipt mismatch")
    except (OSError, ValueError, KeyError, TypeError):
        raise RuntimeError(f"Rust cache verification failed at {entry}; preserve it for inspection and select a new GRAPH_RUST_CACHE_HOME") from None
    return True


def ensure_binary(skill, command):
    if command not in COMMANDS:
        raise RuntimeError("Expected graphctl or helper-register")
    skill = skill.resolve(strict=True)
    key = source_key(skill)
    root = cache_root(skill)
    entry = root / key
    if verified_cache(entry, key):
        return binaries(entry)[command]
    for tool in ("cargo", "rustc"):
        if shutil.which(tool) is None:
            raise RuntimeError(f"{tool} is required for the first Rust graph run; install Rust >=1.85 from https://rustup.rs, then retry")
    root.mkdir(parents=True, exist_ok=True)
    lock = root / (key + ".lock")
    try:
        lock.mkdir()
    except FileExistsError:
        raise RuntimeError(f"Rust graph build is already locked at {lock}; retry after it finishes. If interrupted, confirm no build is running before removing this lock") from None
    try:
        if verified_cache(entry, key):
            return binaries(entry)[command]
        entry.mkdir(exist_ok=True)
        package = skill / "rust-source" / PACKAGE
        artifacts = entry / "artifacts"
        environment = os.environ.copy()
        environment.update(GRAPH_RUST_RUN_ARTIFACT_ROOT=str(artifacts),
                           CARGO_TARGET_DIR=str(entry / "target"),
                           CARGO_HOME=str(entry / "cargo-home"))
        subprocess.run([sys.executable, str(package / "tools/restore_unicode.py"),
                        "--artifact-root", str(artifacts)], check=True,
                       stdout=sys.stderr, env=environment)
        subprocess.run(["cargo", "build", "--locked", "--release", "--manifest-path",
                        str(package / "Cargo.toml"), "--bin", "graphctl", "--bin", "helper-register"],
                       check=True, stdout=sys.stderr, env=environment)
        for binary in binaries(entry).values():
            subprocess.run([str(binary), "--help"], check=True,
                           stdout=subprocess.DEVNULL, env=environment)
        if source_key(skill) != key:
            raise RuntimeError("Bundled Rust source changed during compilation; retry with a stable skill installation")
        receipt = {"source_key": key, "binaries": {name: sha256(path) for name, path in binaries(entry).items()}}
        pending = entry / "complete.pending.json"
        pending.write_text(json.dumps(receipt, sort_keys=True) + "\n", encoding="utf-8")
        pending.replace(entry / "complete.json")
        return binaries(entry)[command]
    finally:
        lock.rmdir()


def main(argv=None):
    args = sys.argv[1:] if argv is None else argv
    if not args or args[0] not in COMMANDS:
        print("Usage: rust-runtime.py {graphctl|helper-register} [arguments...]", file=sys.stderr)
        return 2
    skill = Path(__file__).resolve().parents[1]
    try:
        binary = ensure_binary(skill, args[0])
        environment = os.environ.copy()
        environment["GRAPH_RUST_SKILL_ROOT"] = str(skill)
        os.execve(str(binary), [str(binary), *args[1:]], environment)
    except (RuntimeError, OSError, subprocess.CalledProcessError) as error:
        print(f"Rust graph runtime failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
