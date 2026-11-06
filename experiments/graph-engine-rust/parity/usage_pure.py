"""Compare retained pure-usage oracle records with one native response stream."""

import argparse
import hashlib
import json
import math
from pathlib import Path


def equal(left, right):
    pending = [(left, right)]
    while pending:
        left, right = pending.pop()
        if type(left) is not type(right):
            return False
        if isinstance(left, dict):
            if left.keys() != right.keys():
                return False
            pending.extend((value, right[key]) for key, value in left.items())
        elif isinstance(left, list):
            if len(left) != len(right):
                return False
            pending.extend(zip(left, right))
        elif isinstance(left, float):
            if not (math.isnan(left) and math.isnan(right)) and left.hex() != right.hex():
                return False
        elif left != right:
            return False
    return True


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--responses", type=Path, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    reference_bytes = args.reference.read_bytes()
    response_bytes = args.responses.read_bytes()
    records = [json.loads(line) for line in reference_bytes.splitlines()]
    responses = [json.loads(line) for line in response_bytes.splitlines()]
    failures = []
    for index, record in enumerate(records):
        actual = responses[index] if index < len(responses) else None
        if not equal(record["expected"], actual):
            failures.append({"index": index, "request": record["request"],
                             "expected": record["expected"], "native": actual})
    if len(responses) > len(records):
        failures.append({"unexpected_responses": responses[len(records):]})
    repo = Path(__file__).resolve().parents[3]
    source = repo / "experiments/graph-engine-rust"
    hashes = {str(path.relative_to(repo)).replace("\\", "/"): hashlib.sha256(path.read_bytes()).hexdigest()
              for path in source.rglob("*") if path.is_file() and
              (path.suffix in {".rs", ".toml", ".manifest", ".sql"} or path.name == "Cargo.lock")}
    report = {"comparisons": len(records), "responses": len(responses), "differences": len(failures),
              "failures": failures, "reference": str(args.reference.resolve()),
              "reference_sha256": hashlib.sha256(reference_bytes).hexdigest(),
              "responses_path": str(args.responses.resolve()),
              "responses_sha256": hashlib.sha256(response_bytes).hexdigest(),
              "native_binary": str(args.binary.resolve()),
              "native_binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(),
              "native_source": hashes,
              "comparison": "exact typed values, float bit spelling, and per-result unsorted Python JSON bytes"}
    args.output.mkdir(parents=True, exist_ok=False)
    (args.output / "report.json").write_text(json.dumps(report, indent=2, ensure_ascii=True) + "\n", encoding="utf-8")
    print(json.dumps({key: report[key] for key in ("comparisons", "responses", "differences")}))
    return bool(failures)


if __name__ == "__main__":
    raise SystemExit(main())
