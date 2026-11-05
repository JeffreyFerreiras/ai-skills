"""Replay immutable parser observations without invoking the Python oracle."""

import argparse
import hashlib
import json
from pathlib import Path


def describe(value, depth=0):
    if isinstance(value, dict):
        return {key: describe(child, depth + 1) for key, child in value.items()} if depth < 2 else {"keys": list(value)}
    if isinstance(value, list):
        return {"count": len(value), "first_schema": describe(value[0], depth + 1) if value else None}
    return {"type": type(value).__name__}


def first_record(value):
    if isinstance(value, dict):
        if "argv" in value:
            return value
        for child in value.values():
            result = first_record(child)
            if result is not None:
                return result
    elif isinstance(value, list):
        for child in value:
            result = first_record(child)
            if result is not None:
                return result
    return None


def inspect(path):
    raw = path.read_bytes()
    value = json.loads(raw)
    print(json.dumps(dict(path=str(path), raw_sha256=hashlib.sha256(raw).hexdigest(),
                          schema=describe(value), first_record=first_record(value)), ensure_ascii=True))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--inspect", type=Path, required=True)
    inspect(parser.parse_args().inspect)


if __name__ == "__main__":
    main()
