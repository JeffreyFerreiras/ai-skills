"""Compare all helper CLI observables without dropping traceback structure."""

import argparse
import base64
import hashlib
import json
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    original = args.report.read_bytes()
    report = json.loads(original)
    if len(report["records"]) != 25 or args.output.exists():
        raise ValueError("INVALID_CLASSIFICATION_SCOPE")
    diagnostics, failures = [], []
    for index, record in enumerate(report["records"]):
        source, native = record["oracle"], record["native"]
        fields = ("exit_code", "timeout", "stdout_base64", "after")
        differences = [field for field in fields if source[field] != native[field]]
        source_error = base64.b64decode(source["stderr_base64"])
        native_error = base64.b64decode(native["stderr_base64"])
        if source_error != native_error:
            differences.append("required_stderr_bytes")
        if differences:
            failures.append({"index": index, "label": record["label"], "required_mismatches": differences})
    result = {"scope": "all exit/timeout/stdout/stderr/state observables; exact raw stderr includes traceback frames, source expressions and carets; no normalization", "report": str(args.report), "report_sha256": hashlib.sha256(original).hexdigest(), "comparisons": 25, "required_mismatches": failures, "diagnostic_only_differences": diagnostics, "raw_exact_matches": 25 - len(failures)}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes((json.dumps(result, sort_keys=True, indent=2) + "\n").encode("ascii"))
    print(json.dumps({"comparisons": 25, "required_mismatches": len(failures), "diagnostic_only_differences": len(diagnostics), "raw_exact_matches": result["raw_exact_matches"]}, sort_keys=True))
    return int(bool(failures))


if __name__ == "__main__":
    raise SystemExit(main())
