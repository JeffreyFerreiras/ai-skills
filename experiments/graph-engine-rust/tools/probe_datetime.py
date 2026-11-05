"""Collect bounded, pure datetime compatibility observations."""

import argparse
import datetime
import hashlib
import json
import pathlib
import sys


def vectors():
    values = [
        "0001-01-01T00:00:00Z", "1970-01-01T00:00:00Z",
        "1969-12-31T23:59:59.999999Z", "2000-02-29T12:34:56Z",
        "1900-02-29T12:34:56Z", "9999-12-31T23:59:59.999999Z",
        "0000-01-01T00:00:00Z", "10000-01-01T00:00:00Z",
        "2024-00-01T00:00:00Z", "2024-13-01T00:00:00Z",
        "2024-01-00T00:00:00Z", "2024-04-31T00:00:00Z",
        "2024-01-01T24:00:00Z", "2024-01-01T00:60:00Z",
        "2024-01-01T00:00:60Z", "2024-01-01T00:00:00Z\n",
        "2024-01-01", "20240101", "2024-W01-1", "2024W011",
        "2024-W53-7", "2020-W53-7", "2024-001", "2024-1-1",
        "", "Z", "2024-01-01T", " 2024-01-01T00:00:00Z",
        None, True, False, 0, 1, 1.5, [], {},
    ]
    for fraction in ("", ".", ",", ".1", ".12", ".123", ".1234",
                     ".12345", ".123456", ".1234567", ",1234567"):
        values.append("2024-01-02T03:04:05" + fraction + "Z")
    for time in ("03", "0304", "030405", "03:04", "03:04:05",
                 "03.5", "03:04.5", "030405.1234567", "03:04:05,2"):
        for offset in ("", "Z", "+00:00", "-05:30", "+01:02:03.4"):
            values.append("2024-01-02T" + time + offset)
    for offset in ("+00", "+0000", "+000000", "-00:00:00.1", "+24:00",
                   "+01:60", "+01:02:60", "+01:02:03.1234567", "+1:00"):
        values.append("2024-01-02T03:04:05" + offset)
    for separator in (" ", "t", "x", "\n", "\x00", "\u03a3", "\U0001f600", "\ud800"):
        values.append("2024-01-02" + separator + "03:04:05Z")
    components = ("2024", "01", "02", "03", "04", "05", "123456")
    for zero in (0x660, 0x6F0, 0x966, 0xFF10, 0x1D7CE):
        for selected in range(len(components)):
            converted = list(components)
            converted[selected] = "".join(chr(zero + int(c)) for c in components[selected])
            year, month, day, hour, minute, second, fraction = converted
            values.append(f"{year}-{month}-{day}T{hour}:{minute}:{second}.{fraction}Z")
    return values


def observe(function, value):
    try:
        result = function(value)
        if isinstance(result, datetime.datetime):
            offset = result.utcoffset()
            return {"kind": "datetime", "fields": [result.year, result.month, result.day,
                    result.hour, result.minute, result.second, result.microsecond],
                    "isoformat": result.isoformat(), "aware": offset is not None,
                    "offset_microseconds": None if offset is None else
                    ((offset.days * 86400 + offset.seconds) * 1000000 + offset.microseconds),
                    "fold": result.fold}
        return {"kind": "integer", "value": result}
    except Exception as error:
        return {"kind": "exception", "class": type(error).__name__, "message": str(error)}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    experiment = pathlib.Path(__file__).resolve().parents[1]
    repository = experiment.parents[1]
    sys.path.insert(0, str(repository / "skills" / "software-engineering-graph"))
    from graph_engine import validator

    values = vectors()
    calls = len(values) * 3
    if calls > 512:
        raise RuntimeError(f"datetime reference bound exceeded before calls: {calls}")
    records = []
    functions = (("timestamp_microseconds", validator.timestamp_microseconds),
                 ("fromisoformat", datetime.datetime.fromisoformat),
                 ("receipt_datetime", lambda value: datetime.datetime.fromisoformat(
                     value.replace("Z", "+00:00"))))
    for name, function in functions:
        for value in values:
            records.append({"operation": name, "input": value, "expected": observe(function, value)})
    args.output.mkdir(parents=True, exist_ok=False)
    bindings = {"python": pathlib.Path(sys.executable), "validator": pathlib.Path(validator.__file__),
                "generator": pathlib.Path(__file__)}
    report = {"calls": calls, "limit": 512, "python_version": sys.version,
              "bindings": {name: {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
                           for name, path in bindings.items()}, "records": records}
    encoded = (json.dumps(report, ensure_ascii=True, indent=2) + "\n").encode("ascii")
    (args.output / "report.json").write_bytes(encoded)
    print(json.dumps({"calls": calls, "sha256": hashlib.sha256(encoded).hexdigest(),
                      "report": str(args.output / "report.json")}))


if __name__ == "__main__":
    main()
