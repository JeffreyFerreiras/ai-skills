#!/usr/bin/env python3
"""Build and install the native graph without modifying the source checkout."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


# Keep the build verifier's frozen manifest as the authority.
MANIFEST_SHA256 = "29b2cdb077bc22a61a9cfc1180062fc5fd4336b5c8169d80a5a44383c4ef703f"
SKILL = Path("skills/software-engineering-graph")
PACKAGE = Path("experiments/graph-engine-rust")


class SetupError(Exception):
    pass


def digest(data):
    return hashlib.sha256(data).hexdigest()


def run(command, **kwargs):
    return subprocess.run(command, check=True, **kwargs)


def verify_manifest(package):
    raw = (package / "compat/unicode-input-manifest.json").read_bytes()
    frozen_bytes(raw, MANIFEST_SHA256)
    manifest = json.loads(raw)
    generator = package / manifest["generator"]["path"]
    frozen_bytes(generator.read_bytes(), manifest["generator"]["sha256"])
    return manifest


def frozen_bytes(raw, expected):
    """Restore historical checkout line endings only when the exact hash proves them."""
    for candidate in (raw, raw.replace(b"\r\n", b"\n").replace(b"\n", b"\r\n")):
        if digest(candidate) == expected:
            return candidate
    raise SetupError("Frozen oracle source checksum mismatch")


def stage_oracle(source, build, manifest):
    revision = manifest["oracle"]["revision"]
    git = ["git", "-C", str(source)]
    available = subprocess.run(git + ["cat-file", "-e", revision + "^{commit}"],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if available.returncode:
        # Fetch into a disposable object store, never alter the user's checkout.
        remote = run(git + ["remote", "get-url", "origin"], capture_output=True,
                     text=True).stdout.strip()
        oracle = build / "oracle.git"
        run(["git", "init", "--bare", str(oracle)], stdout=subprocess.DEVNULL)
        git = ["git", "--git-dir", str(oracle)]
        run(git + ["fetch", "--depth", "1", "--no-tags", remote, revision])
    destination = build / SKILL / "graph_engine"
    destination.mkdir(parents=True)
    for name, expected in manifest["oracle"]["modules"].items():
        raw = run(git + ["show", f"{revision}:{SKILL.as_posix()}/graph_engine/{name}"],
                  capture_output=True).stdout
        (destination / name).write_bytes(frozen_bytes(raw, expected))


def prepare_input(package, artifact_root, manifest, supplied):
    target = artifact_root / manifest["input"]["path"]
    if supplied is None:
        run([sys.executable, str(package / "tools/restore_unicode.py"),
             "--artifact-root", str(artifact_root)])
    else:
        raw = supplied.read_bytes()
        if len(raw) != manifest["input"]["size_bytes"] or digest(raw) != manifest["input"]["sha256"]:
            raise SetupError("Unicode input checksum or size mismatch")
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(raw)
    raw = target.read_bytes()
    if len(raw) != manifest["input"]["size_bytes"] or digest(raw) != manifest["input"]["sha256"]:
        raise SetupError("Restored Unicode input checksum or size mismatch")


def native_dispatch(staged, suffix):
    for name in ("graphctl", "helper-register"):
        (staged / "scripts" / (name + ".py")).write_text(
            '#!/usr/bin/env python3\n"""Compatibility launcher for the installed Rust executable."""\n'
            'import os\nfrom pathlib import Path\nimport sys\n'
            f'binary = Path(__file__).with_name({name + suffix!r})\n'
            'os.execv(str(binary), [str(binary), *sys.argv[1:]])\n', encoding="utf-8")
    for document in [staged / "SKILL.md", *(staged / "references").glob("*.md")]:
        text = document.read_text(encoding="utf-8")
        for name in ("graphctl", "helper-register"):
            text = text.replace(f"python <skill>/scripts/{name}.py", f"<skill>/scripts/{name}{suffix}")
        document.write_text(text, encoding="utf-8")


def install(source, destination, supplied=None):
    source = source.resolve(strict=True)
    if destination.is_symlink():
        raise SetupError("Install destination is a symlink; choose a new directory (nothing was replaced)")
    destination = destination.resolve()
    for copied_tree in (source / SKILL, source / PACKAGE):
        if destination == copied_tree or copied_tree in destination.parents:
            raise SetupError("Install destination must be outside the copied skill and Rust source trees")
    if destination.exists() or destination.is_symlink():
        raise SetupError("Install destination already exists; choose a new directory (nothing was replaced)")
    for command in ("git", "cargo", "rustc"):
        if shutil.which(command) is None:
            raise SetupError(f"{command} is required; install Rust >=1.85 from https://rustup.rs and Git, then retry")
    manifest = verify_manifest(source / PACKAGE)
    destination.parent.mkdir(parents=True, exist_ok=True)
    # Stage beside the destination so publication is a single filesystem rename.
    with tempfile.TemporaryDirectory(prefix=".graph-rust-setup-", dir=destination.parent) as temporary:
        work = Path(temporary)
        build = work / "source"
        package = build / PACKAGE
        shutil.copytree(source / PACKAGE, package, ignore=shutil.ignore_patterns("target", "__pycache__"))
        manifest_path = package / "compat/unicode-input-manifest.json"
        manifest_path.write_bytes(frozen_bytes(manifest_path.read_bytes(), MANIFEST_SHA256))
        generator = package / manifest["generator"]["path"]
        generator.write_bytes(frozen_bytes(generator.read_bytes(), manifest["generator"]["sha256"]))
        stage_oracle(source, build, manifest)
        artifacts = work / "artifacts"
        prepare_input(package, artifacts, manifest, supplied)
        environment = os.environ.copy()
        environment.update(GRAPH_RUST_RUN_ARTIFACT_ROOT=str(artifacts),
                           CARGO_TARGET_DIR=str(work / "target"),
                           CARGO_HOME=str(work / "cargo-home"))
        run(["cargo", "build", "--locked", "--release", "--manifest-path",
             str(package / "Cargo.toml"), "--bin", "graphctl", "--bin", "helper-register"],
            env=environment)
        staged = work / "installed"
        shutil.copytree(source / SKILL, staged, ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
        shutil.copytree(package / "licenses", staged / "licenses", dirs_exist_ok=True)
        suffix = ".exe" if os.name == "nt" else ""
        native_dispatch(staged, suffix)
        for name in ("graphctl", "helper-register"):
            binary = staged / "scripts" / (name + suffix)
            shutil.copy2(work / "target/release" / (name + suffix), binary)
            run([str(binary), "--help"], stdout=subprocess.DEVNULL)
        if destination.exists() or destination.is_symlink():
            raise SetupError("Install destination appeared during build; nothing was replaced")
        staged.rename(destination)
    return destination


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True, help="ai-skills Git checkout")
    parser.add_argument("--install-dir", type=Path, required=True, help="new skill directory; never overwrites")
    parser.add_argument("--unicode-input", type=Path, help="optional exact frozen Unicode blob; otherwise restore and verify")
    args = parser.parse_args(argv)
    try:
        destination = install(args.source, args.install_dir, args.unicode_input)
    except (SetupError, OSError, subprocess.CalledProcessError) as error:
        print(f"Rust graph setup failed: {error}", file=sys.stderr)
        return 1
    print(f"Rust graph installed: {destination}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
