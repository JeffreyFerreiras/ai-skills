"""Explicit ISO lease vectors. Reference mode requires separate authorization."""

import argparse
import hashlib
import json
import struct
import sys
from pathlib import Path

FIXED_MICROSECONDS = 1793703600123456
MAX_CASES = 192


def typed(kind, value=None):
    return {"t": kind, "v": value}


def corpus():
    cases = []

    def add(name, value, clock="fixed"):
        cases.append({"id": name, "value": value, "clock": clock,
                      "now_microseconds": str(FIXED_MICROSECONDS)})

    strings = [
        ("boundary-before", "2026-11-03T11:00:00.123455Z"),
        ("boundary-equal", "2026-11-03T11:00:00.123456Z"),
        ("boundary-after", "2026-11-03T11:00:00.123457Z"),
        ("calendar-basic", "20261103T110000.123456+0000"),
        ("week-extended", "2026-W45-2T11:00:00,123456+00:00"),
        ("week-basic", "2026W452T110000.123456+0000"),
        ("week-default-day", "2026-W45T11:00:00Z"),
        ("week-ambiguous", "2026-W45-1123+00"),
        ("hour-only", "2026-11-03T11+00"),
        ("minute-only", "2026-11-03T11:00+00"),
        ("minute-basic", "20261103T1100+0000"),
        ("truncated-colon", "2026-11-03T12:+00"),
        ("fraction-hour", "2026-11-03T11.123456+00"),
        ("fraction-minute", "2026-11-03T11:00.123456+00"),
        ("fraction-long", "2026-11-03T11:00:00.123456789Z"),
        ("fraction-one", "2026-11-03T11:00:00.1Z"),
        ("offset-zero-fraction", "2026-11-03T11:00:00.123456+00:00:00.999999"),
        ("offset-negative-zero-fraction", "2026-11-03T11:00:00.123456-00:00:00.999999"),
        ("offset-positive-microsecond", "2026-11-03T11:00:01.123457+00:00:01.000001"),
        ("offset-negative-microsecond", "2026-11-03T10:59:59.123455-00:00:01.000001"),
        ("offset-normalized-minute", "2026-11-03T12:39:00.123456+00:99"),
        ("offset-normalized-second", "2026-11-03T11:01:39.123456+00:00:99"),
        ("offset-last-positive", "2026-11-03T11:00:00+23:59:59.999999"),
        ("offset-last-negative", "2026-11-03T11:00:00-23:59:59.999999"),
        ("year-first-offset", "0001-01-01T00:00:00+23:59"),
        ("year-last-offset", "9999-12-31T23:59:59.999999-23:59"),
        ("leap-date", "2024-02-29T11:00:00Z"),
        ("naive-date", "2026-11-03"), ("naive-basic-date", "20261103"),
        ("naive-week", "2026-W45-2"), ("naive-time", "2026-11-03T11:00:00.123456"),
        ("invalid-empty", ""), ("invalid-short", "2026"),
        ("invalid-date", "2026-02-29T11:00Z"), ("invalid-month", "2026-13-03T11:00Z"),
        ("invalid-day", "2026-11-31T11:00Z"), ("invalid-year-zero", "0000-01-01T11:00Z"),
        ("invalid-wide-year", "10000-01-01T11:00Z"),
        ("invalid-week-zero", "2026-W00-1T11:00Z"), ("invalid-week-high", "2026-W54-1T11:00Z"),
        ("invalid-week-day", "2026-W45-8T11:00Z"),
        ("invalid-hour", "2026-11-03T24:00Z"), ("invalid-minute", "2026-11-03T11:60Z"),
        ("invalid-second", "2026-11-03T11:00:60Z"),
        ("invalid-offset-positive", "2026-11-03T11:00+24:00"),
        ("invalid-offset-negative", "2026-11-03T11:00-24:00"),
        ("invalid-offset-normalized", "2026-11-03T11:00+23:99"),
        ("invalid-offset-wide", "2026-11-03T11:00+100:00"),
        ("invalid-fraction-empty", "2026-11-03T11:00:00.Z"),
        ("invalid-fraction-tail", "2026-11-03T11:00:00.123xZ"),
        ("invalid-lower-z", "2026-11-03T11:00z"),
        ("invalid-z-tail", "2026-11-03T11:00Ztail"),
        ("invalid-double-z", "2026-11-03T11:00ZZ"),
        ("invalid-leading-space", " 2026-11-03T11:00Z"),
        ("invalid-trailing-space", "2026-11-03T11:00Z "),
        ("invalid-unicode-digit", "٢٠٢٦-11-03T11:00Z"),
        ("embedded-nul-before-offset", "2026-11-03T11:00:00\x00+00"),
        ("embedded-nul-tail", "2026-11-03T11:00:00+00\x00tail"),
    ]
    strings.extend((f"separator-{index}", "2026-11-03" + separator + "11:00:00.123456Z")
                   for index, separator in enumerate((" ", "x", "\t", "é", "😀", "\ud800", "\udfff", "Z")))
    strings.extend((f"surrogate-position-{position}", value[:position] + "\ud800" + value[position + 1:])
                   for position, value in ((0, "2026-11-03T11:00Z"), (7, "2026W452T110000Z"),
                                           (8, "20261103T110000Z"), (12, "2026-11-03T11:00Z")))
    for name, value in strings:
        add(name, typed("str", value))
    for name, value in [
        ("null", typed("null")), ("false", typed("bool", False)), ("true", typed("bool", True)),
        ("zero-int", typed("int", "0")), ("one-int", typed("int", "1")),
        ("huge-int", typed("int", "1" + "0" * 100)),
        ("positive-zero", typed("float", "0000000000000000")),
        ("negative-zero", typed("float", "8000000000000000")),
        ("nan", typed("float", "7ff8000000000001")),
        ("infinity", typed("float", "7ff0000000000000")),
        ("empty-bytes", typed("bytes", "")), ("nonempty-bytes", typed("bytes", "78")),
        ("empty-list", typed("list", [])), ("nonempty-list", typed("list", [typed("int", "1")])),
        ("empty-tuple", typed("tuple", [])), ("nonempty-tuple", typed("tuple", [typed("int", "1")])),
        ("empty-dict", typed("dict", [])), ("nonempty-dict", typed("dict", [[typed("str", "x"), typed("int", "1")]])),
    ]:
        add(name, value)
    for name, value in [("clock-invalid", typed("str", "invalid")), ("clock-falsy", typed("list", [])),
                        ("clock-aware", typed("str", "2026-11-03T11:00:00.123456Z")),
                        ("clock-naive", typed("str", "2026-11-03T11:00:00.123456")),
                        ("clock-nonstring", typed("int", "1"))]:
        add(name, value, "raise")
    assert len(cases) <= MAX_CASES and len({case["id"] for case in cases}) == len(cases)
    return cases


def decode(value):
    kind, payload = value["t"], value["v"]
    if kind == "null": return None
    if kind in {"str", "bool"}: return payload
    if kind == "int": return int(payload)
    if kind == "float": return struct.unpack(">d", bytes.fromhex(payload))[0]
    if kind == "bytes": return bytes.fromhex(payload)
    if kind == "list": return [decode(item) for item in payload]
    if kind == "tuple": return tuple(decode(item) for item in payload)
    if kind == "dict": return {decode(key): decode(item) for key, item in payload}
    raise AssertionError(kind)


def prepare(directory):
    cases = corpus()
    lines = [json.dumps(case, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode("ascii") + b"\n" for case in cases]
    requests = directory / "requests.jsonl"
    with requests.open("xb") as stream:
        stream.writelines(lines)
    manifest = {"cases": len(cases), "max_cases": MAX_CASES, "fixed_microseconds": str(FIXED_MICROSECONDS),
                "cases_sha256": [{"id": case["id"], "sha256": hashlib.sha256(line).hexdigest()} for case, line in zip(cases, lines)],
                "limitations": ["selected CPython and fixed aware UTC clock only", "selected built-in value types; no user-defined replace/comparison methods", "function return/error and clock ordering; no CLI traceback workload", "source-derived finite vector selection, not exhaustive ISO grammar"]}
    (directory / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return manifest


def reference(requests, source_root):
    # No original handler, state store, configured check, Git or subprocess is called.
    from datetime import datetime, timezone
    sys.path.insert(0, str(source_root))
    from graph_engine import cli
    cases = [json.loads(line) for line in requests.read_bytes().splitlines()]
    assert cases == corpus() and len(cases) <= MAX_CASES  # Before the first source call.
    fixed = datetime(2026, 11, 3, 11, 0, 0, 123456, tzinfo=timezone.utc)

    class LeaseClockError(Exception):
        pass

    class Clock:
        fromisoformat = staticmethod(datetime.fromisoformat)

        @staticmethod
        def now(tz):
            assert tz is timezone.utc
            reads[0] += 1
            if case["clock"] == "raise": raise LeaseClockError("clock sentinel")
            return fixed

    cli.datetime = Clock
    for case in cases:
        reads = [0]
        assert case["now_microseconds"] == str(FIXED_MICROSECONDS)
        value = decode(case["value"])
        try:
            outcome = {"ok": cli._lease_expired(value)}
        except Exception as error:
            outcome = {"error": {"kind": type(error).__name__, "message": str(error)}}
        result = {"id": case["id"], "outcome": outcome, "clock_reads": reads[0]}
        sys.stdout.buffer.write(json.dumps(result, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode("ascii") + b"\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=("prepare", "reference"))
    parser.add_argument("path", type=Path)
    parser.add_argument("source_root", nargs="?", type=Path)
    args = parser.parse_args()
    if args.mode == "prepare": prepare(args.path)
    else:
        assert args.source_root is not None
        reference(args.path, args.source_root)
