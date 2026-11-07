"""Compare usage API outcomes and selected exact SQL bytes; retain clock diagnostics."""

import argparse
import hashlib
import json
from pathlib import Path

from usage_pure import equal


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--native", type=Path, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    reference_bytes, native_bytes = args.reference.read_bytes(), args.native.read_bytes()
    reference, native = json.loads(reference_bytes), json.loads(native_bytes)
    failures = []
    comparisons = 0
    if reference["seed_sha256"] != native["seed_sha256"]:
        raise RuntimeError("native and oracle seed bindings differ")
    if len(reference["records"]) != len(native["records"]):
        failures.append({"scenario_counts": [len(reference["records"]), len(native["records"])]})
    for expected_case, actual_case in zip(reference["records"], native["records"]):
        if expected_case["name"] != actual_case["name"]:
            failures.append({"scenario_order": [expected_case["name"], actual_case["name"]]})
            continue
        if len(expected_case["rows"]) != len(actual_case["rows"]):
            failures.append({"scenario": expected_case["name"], "row_counts": [len(expected_case["rows"]), len(actual_case["rows"])]})
        for index, (expected, actual) in enumerate(zip(expected_case["rows"], actual_case["rows"])):
            comparisons += 1
            if not equal(expected, actual):
                failures.append({"scenario": expected_case["name"], "step_index": index,
                                 "step": expected["step"], "expected": expected, "native": actual})
    source = Path(__file__).resolve().parents[1]
    source_hashes = {str(path.relative_to(source)).replace("\\", "/"): hashlib.sha256(path.read_bytes()).hexdigest()
                     for path in source.rglob("*") if path.is_file() and
                     (path.suffix in {".rs", ".toml", ".manifest", ".sql"} or path.name == "Cargo.lock")}
    binary_hash = hashlib.sha256(args.binary.read_bytes()).hexdigest()
    if binary_hash != native["native_binary_sha256"]:
        raise RuntimeError("native binary changed since execution")
    report = {"scenarios": len(reference["records"]), "step_comparisons": comparisons,
              "differences": len(failures), "failures": failures,
              "reference": str(args.reference.resolve()), "reference_sha256": hashlib.sha256(reference_bytes).hexdigest(),
              "native": str(args.native.resolve()), "native_sha256": hashlib.sha256(native_bytes).hexdigest(),
              "native_binary": str(args.binary.resolve()), "native_binary_sha256": binary_hash,
              "native_source": source_hashes, "seed_sha256": reference["seed_sha256"],
              "matched": "exact typed outcomes, unsorted per-result JSON bytes, revision/status, operation digests/response bytes, event details/order",
              "diagnostics_retained_separately": "owned fixture paths and actual default-clock columns in both immutable input reports",
              "proof_limit": "backend component only; private harness no-op semantic callback; no full GraphCase/CLI/core-semantic or whole DB file equivalence claim"}
    args.output.mkdir(parents=True, exist_ok=False)
    (args.output / "report.json").write_text(json.dumps(report, ensure_ascii=True, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({key: report[key] for key in ("scenarios", "step_comparisons", "differences")}))
    return bool(failures)


if __name__ == "__main__":
    raise SystemExit(main())
