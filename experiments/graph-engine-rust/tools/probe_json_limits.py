"""Observe native CPython JSON boundaries without changing interpreter settings."""

import argparse
import json
import sys
from pathlib import Path


def attempt(depth, kind, operation):
    try:
        if operation == "loads":
            text = "["*depth+"0"+"]"*depth if kind == "array" else '{"a":'*depth+"0"+"}"*depth
            result = json.loads(text)
        else:
            result = 0
            for _ in range(depth):
                result = [result] if kind == "array" else {"a": result}
            result = json.dumps(result, sort_keys=True, separators=(",", ":"))
        del result
        return {"ok": True}
    except (ValueError, RecursionError) as error:
        return {"ok": False, "kind": type(error).__name__, "message": str(error)}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    results = []
    for operation in ("loads", "dumps"):
        for kind in ("array", "object"):
            low, high = 0, 10000
            while low+1 < high:
                middle = (low+high)//2
                if attempt(middle, kind, operation)["ok"]:
                    low = middle
                else:
                    high = middle
            results.append(dict(operation=operation, kind=kind, accepted=low,
                                first_rejected=high, accepted_result=attempt(low, kind, operation),
                                rejected_result=attempt(high, kind, operation)))
    report = {"python": sys.version, "recursion_limit_setting": sys.getrecursionlimit(),
              "scope": "this explicit probe call boundary, not an assumption about every engine ingress",
              "results": results}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2)+"\n", encoding="utf-8")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
