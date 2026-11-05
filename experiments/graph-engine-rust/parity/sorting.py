"""Prepare typed ordering vectors; reference mode requires separate authorization."""

import argparse
import hashlib
import json
import struct
import sys
from pathlib import Path


MAX_CASES = 770
MAX_ITEMS = 1024
MAX_COMPARISONS_PER_CASE = 100_000
MAX_TOTAL_COMPARISONS = 2_000_000
SORT_PATTERNS = (
    "ascending", "descending", "alternating", "organpipe", "duplicates",
    "ragged_runs", "leading_nan", "trailing_nan", "middle_nan", "alternating_nan",
    "nan_runs", "mixed_numeric", "infinities", "signed_zero", "constant_nan_lists",
    "independent_nan_lists",
)
DICT_PATTERNS = (
    "descending_int", "mixed_numeric", "leading_nan", "trailing_nan",
    "alternating_nan", "constant_nan", "unicode", "bool_collision",
)
PROFILES = ("canonical", "cli", "stored", "sorted_stored")


def integer(value):
    return {"t": "int", "v": str(value)}


def floating(value):
    return {"t": "float", "v": struct.pack(">d", value).hex()}


def text(value):
    return {"t": "str", "v": value}


def nan():
    return {"t": "float", "v": "7ff8000000000001"}


def keys(pattern, size):
    result = []
    for i in range(size):
        if pattern in {"ascending", "descending_int"}:
            value = integer(i if pattern == "ascending" else size - i)
        elif pattern == "descending":
            value = integer(size - i)
        elif pattern == "alternating":
            value = integer(i // 2 if i % 2 else size - i // 2)
        elif pattern == "organpipe":
            value = integer(min(i, size - i))
        elif pattern == "duplicates":
            value = integer((i * 37) % 11)
        elif pattern == "ragged_runs":
            value = integer((size - i // 17) * 31 + i % 17)
        elif pattern in {"leading_nan", "trailing_nan", "middle_nan"}:
            position = {"leading_nan": 0, "trailing_nan": size - 1,
                        "middle_nan": size // 2}[pattern]
            value = nan() if i == position else integer(size - i)
        elif pattern == "alternating_nan":
            value = nan() if i % 2 else integer(size - i)
        elif pattern == "nan_runs":
            value = nan() if (i // 13) % 3 == 1 else integer((size - i) % 53)
        elif pattern == "mixed_numeric":
            value = (integer((size - i) % 17), floating(float((size - i) % 19)),
                     {"t": "bool", "v": bool(i % 2)})[i % 3]
        elif pattern == "infinities":
            value = floating((float("inf"), float("-inf"), float(i))[i % 3])
        elif pattern == "signed_zero":
            value = floating(-0.0 if i % 2 else 0.0)
        elif pattern in {"constant_nan_lists", "independent_nan_lists"}:
            first = {"t": "constant_nan"} if pattern == "constant_nan_lists" else nan()
            value = {"t": "list", "v": [first, integer(size - i)]}
        elif pattern == "constant_nan":
            value = {"t": "constant_nan"} if i % 2 else integer(size - i)
        elif pattern == "unicode":
            value = text(("\U0001f600" if i % 2 else "a") + chr(0x400 + size - i))
        elif pattern == "bool_collision":
            value = integer(i % 2) if i % 3 else {"t": "bool", "v": bool(i % 2)}
        else:
            raise AssertionError(pattern)
        result.append(value)
    return result


def dictionary(items):
    return {"t": "dict", "v": [[key, integer(i)] for i, key in enumerate(items)]}


def corpus():
    cases = []
    for size in (0, 1, 2, 31, 32, 63, 64, 65, 127, 128, 129, 257, 1024):
        for pattern in SORT_PATTERNS:
            cases.append({"id": f"sort-{pattern}-{size}", "op": "sort_trace",
                          "keys": keys(pattern, size), "fail_at": 0})
    bad_keys = (text("x"), {"t": "null"}, {"t": "bytes", "v": "78"},
                {"t": "list", "v": []})
    for size in (2, 33, 65, 129, 257):
        for bad_index, bad in enumerate(bad_keys):
            for position_index, position in enumerate((0, size // 2, size - 1)):
                values = keys("ragged_runs", size)
                values[position] = bad
                cases.append({"id": f"sort-type-error-{size}-{bad_index}-{position_index}-{position}",
                              "op": "sort_trace", "keys": values, "fail_at": 0})
    for pattern in ("descending", "ragged_runs", "leading_nan", "middle_nan",
                    "mixed_numeric", "constant_nan_lists"):
        for fail_at in (1, 2, 3, 7, 31, 63, 127, 255, 511):
            cases.append({"id": f"sort-injected-{pattern}-{fail_at}", "op": "sort_trace",
                          "keys": keys(pattern, 257), "fail_at": fail_at})
    for size in (0, 1, 2, 31, 32, 63, 64, 65, 127, 129, 257):
        for pattern in DICT_PATTERNS:
            for profile in PROFILES:
                cases.append({"id": f"dictionary-{pattern}-{size}-{profile}",
                              "op": "private_sorted_json", "profile": profile,
                              "dictionary": dictionary(keys(pattern, size))})
    special = (
        [], [{"t": "null"}], [{"t": "bytes", "v": "78"}],
        [{"t": "list", "v": []}], [{"t": "object"}],
        [text("x"), integer(1)], [integer(1), text("x")],
        [{"t": "null"}, integer(1)], [integer(1), {"t": "null"}],
        [{"t": "bytes", "v": "78"}, text("x")],
        [text("x"), {"t": "bytes", "v": "78"}],
        [{"t": "bool", "v": False}, {"t": "null"}],
        [text("x"), text("\ud800")], [nan(), integer(1)],
        [{"t": "constant_nan"}, nan()],
        [integer(10 ** 4000), floating(float("inf"))],
    )
    for number, items in enumerate(special):
        for profile in PROFILES:
            cases.append({"id": f"dictionary-special-{number}-{profile}",
                          "op": "private_sorted_json", "profile": profile,
                          "dictionary": dictionary(items)})
    nested = (
        dictionary(keys("descending_int", 129)),
        dictionary([integer(1), text("bad")]),
        {"t": "list", "v": [dictionary(keys("alternating_nan", 129))]},
        {"t": "bytes", "v": "ff"}, text("\ud800"),
        {"t": "dict", "v": [[text("\ud800"), integer(1)]]},
        integer(10 ** 4000),
        {"t": "list", "v": [nan(), {"t": "constant_nan"}, floating(-0.0)]},
    )
    for number, value in enumerate(nested):
        for profile in PROFILES:
            cases.append({"id": f"dictionary-nested-{number}-{profile}",
                          "op": "private_sorted_json", "profile": profile,
                          "dictionary": {"t": "dict", "v": [[text("z"), integer(0)],
                                                                  [text("a"), value]]}})
    assert len(cases) == MAX_CASES
    assert len({case["id"] for case in cases}) == MAX_CASES
    assert all(len(case.get("keys", ())) <= MAX_ITEMS for case in cases)
    return cases


def prepare(requests, manifest):
    assert not requests.exists() and not manifest.exists(), "one-shot output guard"
    cases = corpus()
    data = b"".join((json.dumps(case, ensure_ascii=True, separators=(",", ":")) + "\n")
                    .encode("ascii") for case in cases)
    requests.write_bytes(data)
    manifest.write_text(json.dumps({
        "cases": len(cases), "sort_cases": sum(c["op"] == "sort_trace" for c in cases),
        "dictionary_cases": sum(c["op"] == "private_sorted_json" for c in cases),
        "requests_sha256": hashlib.sha256(data).hexdigest(),
        "max_items": MAX_ITEMS, "max_comparisons_per_case": MAX_COMPARISONS_PER_CASE,
        "max_total_comparisons": MAX_TOTAL_COMPARISONS,
        "case_ids": [case["id"] for case in cases],
        "reference_invoked": False,
    }, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"requests": str(requests), "manifest": str(manifest), "cases": len(cases)}))


def decode(spec):
    kind = spec["t"]
    if kind == "null":
        return None
    if kind == "bool":
        return spec["v"]
    if kind == "int":
        return int(spec["v"])
    if kind == "float":
        return struct.unpack(">d", bytes.fromhex(spec["v"]))[0]
    if kind == "constant_nan":
        return json.decoder.NaN
    if kind == "str":
        return spec["v"]
    if kind == "bytes":
        return bytes.fromhex(spec["v"])
    if kind == "list":
        return [decode(item) for item in spec["v"]]
    if kind == "tuple":
        return tuple(decode(item) for item in spec["v"])
    if kind == "object":
        return {key: decode(value) for key, value in spec.get("v", {}).items()}
    if kind == "dict":
        result = {}
        for key, value in spec["v"]:
            # Match evaluation order of the native owned dictionary constructor.
            key, value = decode(key), decode(value)
            result[key] = value
        return result
    raise AssertionError(kind)


def error_descriptor(error):
    return {"kind": type(error).__name__, "message": str(error),
            "position": getattr(error, "start", 0)}


def reference(requests, oracle_root):
    # Preparation and metadata inspection never import frozen engine code.
    sys.path.insert(0, str(oracle_root))
    from graph_engine.ids import canonical_bytes

    cases = [json.loads(line) for line in requests.read_bytes().splitlines()]
    assert len(cases) == MAX_CASES  # Before the first target operation.
    assert len({case["id"] for case in cases}) == MAX_CASES
    assert all(len(case.get("keys", ())) <= MAX_ITEMS for case in cases)
    total_comparisons = 0
    for case in cases:
        try:
            if case["op"] == "sort_trace":
                values = [decode(key) for key in case["keys"]]
                trace = []

                class Item:
                    def __init__(self, index):
                        self.index = index

                    def __lt__(self, other):
                        nonlocal total_comparisons
                        trace.append([self.index, other.index])
                        total_comparisons += 1
                        assert total_comparisons <= MAX_TOTAL_COMPARISONS
                        if len(trace) == case["fail_at"]:
                            raise TypeError("injected comparison failure")
                        if len(trace) > MAX_COMPARISONS_PER_CASE:
                            raise RuntimeError("verification comparison bound exceeded")
                        return values[self.index] < values[other.index]

                items = [Item(i) for i in range(len(values))]
                error = None
                try:
                    items.sort()
                except (TypeError, ValueError, OverflowError, RuntimeError) as failure:
                    error = error_descriptor(failure)
                result = {"permutation": [item.index for item in items],
                          "trace": trace, "error": error}
            else:
                value = decode(case["dictionary"])
                profile = case["profile"]
                if profile == "canonical":
                    data = canonical_bytes(value)
                else:
                    data = json.dumps(value, sort_keys=profile != "stored",
                                      separators=(",", ":") if profile == "cli" else None)
                    data = data.encode("utf-8")
                result = data.hex()
            response = {"ok": True, "result": result}
        except (TypeError, ValueError, OverflowError, UnicodeError, RecursionError) as failure:
            response = {"ok": False, "error": error_descriptor(failure)}
        data = json.dumps(response, sort_keys=True, separators=(",", ":")).encode("utf-8")
        sys.stdout.buffer.write(data + b"\n")
        sys.stdout.buffer.flush()


def main():
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    prepare_parser = subparsers.add_parser("prepare")
    prepare_parser.add_argument("requests", type=Path)
    prepare_parser.add_argument("manifest", type=Path)
    reference_parser = subparsers.add_parser("reference")
    reference_parser.add_argument("requests", type=Path)
    reference_parser.add_argument("oracle_root", type=Path)
    args = parser.parse_args()
    if args.command == "prepare":
        prepare(args.requests, args.manifest)
    else:
        reference(args.requests, args.oracle_root)


if __name__ == "__main__":
    main()
