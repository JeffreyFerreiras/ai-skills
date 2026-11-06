"""Record exact source, dependency, input and binary identities for check evidence."""

import hashlib
import json
import os
import platform
import subprocess
from pathlib import Path


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def capture(binary):
    package = Path(__file__).resolve().parent.parent
    root = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve()
    manifest = package / "compat/unicode-input-manifest.json"
    source_files = [path for path in package.rglob("*")
                    if path.is_file() and path.suffix in {".rs", ".toml", ".lock", ".py", ".json", ".sql", ".manifest"}]
    return {
        "source_files_sha256": {path.relative_to(package).as_posix(): sha256(path) for path in sorted(source_files)},
        "binary_path": str(binary.resolve()), "binary_sha256": sha256(binary),
        "cargo_lock_sha256": sha256(package / "Cargo.lock"),
        "unicode_manifest_sha256": sha256(manifest),
        "unicode_input_sha256": sha256(root / json.loads(manifest.read_text(encoding="utf-8"))["input"]["path"]),
        "rustc_verbose": subprocess.check_output(["rustc", "--version", "--verbose"], text=True),
        "cargo_version": subprocess.check_output(["cargo", "--version"], text=True).strip(),
        "platform": platform.platform(),
        "environment": {name: os.environ.get(name) for name in
                        ("CARGO_HOME", "CARGO_TARGET_DIR", "GRAPH_RUST_RUN_ARTIFACT_ROOT", "PYTHONDONTWRITEBYTECODE", "TEMP", "TMP")},
    }
