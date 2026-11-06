"""Exercise preparation and compile-time validation on disposable ART fixtures."""

import argparse
import hashlib
import json
import os
import shutil
import struct
from pathlib import Path
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "parity"))
from workload_io import passed, raw, run_child, under_root


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    package = Path(__file__).resolve().parent.parent
    root = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve()
    output = args.output.resolve()
    if not output.is_relative_to(root) or output.exists():
        parser.error("a new fixture directory below GRAPH_RUST_RUN_ARTIFACT_ROOT is required")
    output.mkdir(parents=True)
    fixtures = under_root(root / "fixtures", root)
    fixtures.mkdir(exist_ok=True)
    fixture = Path(tempfile.mkdtemp(prefix="uc", dir=fixtures))
    source = fixture / "repo/experiments/graph-engine-rust"
    shutil.copytree(package, source)
    oracle = package.parent.parent / "skills/software-engineering-graph/graph_engine"
    shutil.copytree(oracle, fixture / "repo/skills/software-engineering-graph/graph_engine", ignore=shutil.ignore_patterns("__pycache__"))
    input_relative = Path("compat/unicode/cpython-3.12.10/unicode-tables.bin")
    fixture_root = fixture / "artifacts"
    table = fixture_root / input_relative
    table.parent.mkdir(parents=True)
    shutil.copyfile(root / input_relative, table)
    target_root = Path(os.environ["CARGO_TARGET_DIR"])
    if not target_root.is_absolute() or not target_root.is_dir():
        raise ValueError("an existing absolute Cargo target root is required")
    targets = Path(tempfile.mkdtemp(prefix="uc", dir=target_root))
    env = dict(os.environ, GRAPH_RUST_RUN_ARTIFACT_ROOT=str(fixture_root), PYTHONDONTWRITEBYTECODE="1")
    manifest = source / "compat/unicode-input-manifest.json"
    generator = source / "tools/generate_unicode.py"
    originals = {path: path.read_bytes() for path in (table, manifest, generator)}
    records = []

    def run(case, expect_success):
        target = targets / str(len(records) // 2)
        case_env = dict(env, CARGO_TARGET_DIR=str(target))
        commands = [[sys.executable, "-B", str(source / "tools/prepare_compat.py"), "verify"],
                    ["cargo", "build", "--locked", "--manifest-path", str(source / "Cargo.toml"), "--lib"]]
        for index, command in enumerate(commands):
            process = run_child(output, f"{case}.{index}", command, source, case_env, timeout=300)
            diagnostic = raw(process, "stderr")
            expected_exit = 0 if expect_success else (1 if index == 0 else 101)
            accepted = passed(process, expected_exit)
            if not expect_success:
                accepted &= b"COMPAT_" in diagnostic or (index == 0 and case.startswith("missing_") and b"FileNotFoundError" in diagnostic)
            record = {"case": case, "process": process, "expected_success": expect_success, "passed": accepted}
            records.append(record)
            if not record["passed"]:
                raise AssertionError(json.dumps(record))
        if not expect_success and any(target.glob("debug/build/graph-engine-rust-*/out/unicode-tables.bin")):
            raise AssertionError("REJECTED_INPUT_REACHED_EMBEDDING")
        print(json.dumps({"case": case, "passed": True}), flush=True)

    run("valid_cold_target", True)
    for case in ("missing_input", "tampered_input", "wrong_input_version", "wrong_input_provenance", "missing_manifest", "tampered_manifest", "missing_generator", "tampered_generator"):
        held = None
        try:
            if case.startswith("missing_"):
                path = {"missing_input": table, "missing_manifest": manifest, "missing_generator": generator}[case]
                held = (path, path.with_name(path.name + ".held"))
                path.rename(held[1])
            elif case == "tampered_input":
                data = bytearray(originals[table]); data[-1] ^= 1; table.write_bytes(data)
            elif case in {"wrong_input_version", "wrong_input_provenance"}:
                data = bytearray(originals[table]); size, = struct.unpack_from("<I", data, 8)
                header = bytes(data[12:12+size])
                if case == "wrong_input_version":
                    header = header.replace(b'"version":1}', b'"version":2}')
                else:
                    header = header.replace(b'"unicode_version":"15.0.0"', b'"unicode_version":"16.0.0"')
                if len(header) != size:
                    raise AssertionError("negative header fixture changed size")
                data[12:12+size] = header; table.write_bytes(data)
            elif case == "tampered_manifest":
                manifest.write_bytes(originals[manifest].replace(b'"version": 1', b'"version": 2'))
            elif case == "tampered_generator":
                generator.write_bytes(originals[generator] + b"\n# tampered fixture\n")
            run(case, False)
        finally:
            if held is not None:
                held[1].rename(held[0])
            for path, data in originals.items():
                if path.read_bytes() != data:
                    path.write_bytes(data)
    run("valid_after_rejections", True)
    report = {"scope": "frozen Unicode input preparation/build rejection", "all_passed": all(r["passed"] for r in records),
              "records": records, "manifest_sha256": digest(manifest),
              "input_sha256": digest(table), "target_cache": "fresh target per case",
              "dependency_registry_cache": "inherited ART CARGO_HOME", "benchmark": False,
              "fixture": str(fixture), "isolated_target_parent": str(targets)}
    (output / "report.json").write_text(json.dumps(report, indent=2)+"\n", encoding="utf-8")


if __name__ == "__main__":
    main()
