"""Prepare104 typed tuple cases; reference mode requires separate authorization."""

import argparse
import hashlib
import json
from pathlib import Path
import struct
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from sorting import decode, error_descriptor, integer, nan, text


CASE_COUNT = 104
PROFILES = ("canonical", "cli", "stored", "sorted_stored")


def tup(*items):
    return {"t": "tuple", "v": list(items)}


def seq(*items):
    return {"t": "list", "v": list(items)}


def obj(**items):
    return {"t": "object", "v": items}


def task(schema=1):
    return obj(schema_version=integer(schema), task_id=text("t"), request_mode=text("delivery"),
               minimum_route=text("fast_path"), mandatory_impact_tags=tup(), risk_level=text("low"),
               acceptance_criteria=tup(), authority=obj(), policy_approval=obj(), evidence_paths=tup(),
               inspection_budget=obj(), required_check_ids=tup(), required_human_decisions=tup(),
               model_sizing=obj(), helper_allowance=obj())


def corpus():
    cases = []
    key_sets = (
        [tup()], [tup(integer(1))], [tup(integer(2)), tup(integer(1), text("x"))],
        [tup({"t": "bool", "v": True}), tup(integer(1))],
        [tup({"t": "constant_nan"}, integer(2)), tup({"t": "constant_nan"}, integer(1))],
        [tup(nan(), integer(2)), tup(nan(), integer(1))],
        [tup(), integer(1)], [integer(1), tup()],
        [tup(text("\ud800")), tup(text("a"))],
        [tup(seq())], [tup(obj())],
        [tup(integer(1), {"t": "null"}), tup(integer(1), text("x"))],
    )
    for number, keys in enumerate(key_sets):
        for profile in PROFILES:
            cases.append({"id": f"tuple-dictionary-{number}-{profile}", "op": "private_sorted_json",
                          "profile": profile, "dictionary": {"t": "dict", "v": [
                              [key, integer(i)] for i, key in enumerate(keys)]}})
    variants = (
        seq(tup(text("x"), integer(1))), tup(seq(text("x"), integer(1))),
        tup(tup(tup(text("slot"), integer(1)), integer(3))),
        tup(tup(tup(seq()), integer(3))),
    )
    for field, schema in (("inspection_budget", 1), ("model_sizing", 2),
                          ("delivery_readiness", 1), ("helper_allowance", 3),
                          ("model_overrides", 1)):
        for number, pairs in enumerate(variants):
            value = task(schema)
            value["v"][field] = obj(chosen=pairs) if field == "model_overrides" else pairs
            cases.append({"id": f"tuple-subset-{field}-{number}", "op": "private_subset", "task": value})
    malformed = []
    value = task()
    value["v"]["schema_version"] = seq()
    malformed.append(value)
    value = task()
    del value["v"]["inspection_budget"]
    malformed.append(value)
    value = task()
    value["v"]["inspection_budget"] = tup(tup(integer(1), integer(2), integer(3)))
    malformed.append(value)
    value = task()
    value["v"]["inspection_budget"] = integer(7)
    malformed.append(value)
    for number, value in enumerate(malformed):
        cases.append({"id": f"tuple-subset-error-{number}", "op": "private_subset", "task": value})
    pairs = {
        "equal": (
            (tup(), tup()), (tup(integer(1)), seq(integer(1))),
            (tup({"t": "bool", "v": True}), tup(integer(1))),
            (tup({"t": "constant_nan"}), tup({"t": "constant_nan"})),
            (tup(nan()), tup(nan())), (tup(tup(text("x"))), tup(tup(text("x")))),
        ),
        "less": (
            (tup(), tup(integer(1))), (tup(integer(1)), tup(integer(1), integer(0))),
            (tup(text("a")), tup(text("\U0001f600"))),
            (tup({"t": "bytes", "v": "00"}), tup({"t": "bytes", "v": "ff"})),
            (tup({"t": "constant_nan"}, integer(1)), tup({"t": "constant_nan"}, integer(2))),
            (tup(nan(), integer(1)), tup(nan(), integer(2))),
            (tup(), seq()), (seq(), tup())),
        "add": ((tup(integer(1)), tup(integer(2))), (tup(), seq()),
                (seq(), tup()), (tup(), text("x"))),
    }
    for operation, values in pairs.items():
        for number, (left, right) in enumerate(values):
            cases.append({"id": f"tuple-{operation}-{number}", "op": "tuple_operation",
                          "operation": operation, "left": left, "right": right})
    unary = {
        "length": (tup(), tup(integer(1)), tup(*(integer(i) for i in range(1024))), tup(text("\U0001f600"))),
        "repr": (tup(), tup(integer(1)), tup(text("\ud800")),
                 tup(seq(integer(1)), tup({"t": "bytes", "v": "ff"}))),
        "hashable": (tup(), tup(integer(1), {"t": "constant_nan"}, {"t": "bytes", "v": "ff"}),
                     tup(tup(seq())), tup(obj())),
        "coerce": (tup(tup(text("x"), integer(1)), seq(text("x"), integer(2))),
                   tup(text("ab"))),
    }
    for operation, values in unary.items():
        for number, value in enumerate(values):
            cases.append({"id": f"tuple-{operation}-{number}", "op": "tuple_operation",
                          "operation": operation, "left": value})
    assert len(cases) == CASE_COUNT
    assert len({case["id"] for case in cases}) == CASE_COUNT
    return cases


def prepare(requests, manifest):
    assert not requests.exists() and not manifest.exists(), "preserve prior artifacts"
    cases = corpus()
    data = b"".join((json.dumps(case, ensure_ascii=True, separators=(",", ":")) + "\n").encode("ascii")
                    for case in cases)
    requests.write_bytes(data)
    manifest.write_text(json.dumps({"cases": CASE_COUNT, "dictionary_cases": 48,
                                   "subset_cases": 24, "tuple_operations": 32,
                                   "requests_sha256": hashlib.sha256(data).hexdigest(),
                                   "case_ids": [case["id"] for case in cases],
                                   "reference_invoked": False}, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"cases": CASE_COUNT, "requests": str(requests), "reference_invoked": False}))


def descriptor(value):
    if value is None:
        return {"t": "null"}
    if type(value) is bool:
        return {"t": "bool", "v": value}
    if type(value) is int:
        return {"t": "int", "v": str(value)}
    if value is json.decoder.NaN:
        return {"t": "constant_nan"}
    if type(value) is float:
        return {"t": "float", "v": struct.pack(">d", value).hex()}
    if type(value) is str:
        return {"t": "str", "v": value}
    if type(value) is bytes:
        return {"t": "bytes", "v": value.hex()}
    if type(value) in (tuple, list):
        return {"t": type(value).__name__, "v": [descriptor(item) for item in value]}
    if type(value) is dict:
        return {"t": "dict", "v": [[descriptor(key), descriptor(item)] for key, item in value.items()]}
    raise AssertionError(type(value))


def reference(requests, oracle_root):
    sys.path.insert(0, str(oracle_root))
    from graph_engine.contracts import authoritative_task_subset
    from graph_engine.ids import canonical_bytes

    cases = [json.loads(line) for line in requests.read_bytes().splitlines()]
    assert len(cases) == CASE_COUNT  # Before the first target function call.
    assert len({case["id"] for case in cases}) == CASE_COUNT
    for case in cases:
        try:
            if case["op"] == "private_subset":
                result = descriptor(authoritative_task_subset(decode(case["task"])))
            elif case["op"] == "private_sorted_json":
                value, profile = decode(case["dictionary"]), case["profile"]
                if profile == "canonical":
                    data = canonical_bytes(value)
                else:
                    data = json.dumps(value, sort_keys=profile != "stored",
                                      separators=(",", ":") if profile == "cli" else None).encode("utf-8")
                result = data.hex()
            else:
                left, right = decode(case["left"]), decode(case.get("right", {"t": "null"}))
                operation = case["operation"]
                if operation == "equal":
                    result = left == right
                elif operation == "less":
                    result = left < right
                elif operation == "add":
                    result = descriptor(left + right)
                elif operation == "length":
                    result = len(left)
                elif operation == "repr":
                    result = repr(left)
                elif operation == "hashable":
                    hash(left)
                    result = True
                elif operation == "coerce":
                    result = descriptor(dict(left))
                else:
                    raise AssertionError(operation)
            response = {"ok": True, "result": result}
        except (TypeError, ValueError, KeyError, AttributeError, OverflowError,
                UnicodeError, RecursionError) as failure:
            response = {"ok": False, "error": error_descriptor(failure)}
        sys.stdout.buffer.write(json.dumps(response, sort_keys=True,
                                           separators=(",", ":")).encode("utf-8") + b"\n")
        sys.stdout.buffer.flush()


def main():
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    for command in ("prepare", "reference"):
        child = subparsers.add_parser(command)
        child.add_argument("requests", type=Path)
        child.add_argument("second_path", type=Path)
    args = parser.parse_args()
    if args.command == "prepare":
        prepare(args.requests, args.second_path)
    else:
        reference(args.requests, args.second_path)


if __name__ == "__main__":
    main()
