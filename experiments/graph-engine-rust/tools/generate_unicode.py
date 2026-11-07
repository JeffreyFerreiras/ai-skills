"""Explicitly produce a candidate compatibility input, never approve or build it."""

import argparse
import hashlib
import json
import struct
import sys
import unicodedata
from pathlib import Path

MAGIC = b"GUNICOD1"
RELATIVE_INPUT = "compat/unicode/cpython-3.12.10/unicode-tables.bin"
ORACLE_REVISION = "5f10521a4a249d03961fe2ad84df64ceed88c7ad"


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def provenance(repo):
    sources = repo / "skills/software-engineering-graph/graph_engine"
    return {
        "implementation": sys.implementation.name,
        "version": ".".join(map(str, sys.version_info[:3])),
        "executable_sha256": sha256(Path(sys.executable)),
        "revision": ORACLE_REVISION,
        "modules": {p.name: sha256(p) for p in sorted(sources.glob("*.py"))},
        "unicode_version": unicodedata.unidata_version,
        "integer_digit_limit": sys.get_int_max_str_digits(),
        "recursion_limit": sys.getrecursionlimit(),
    }


def generate(repo, output):
    observed = provenance(repo)
    if (observed["implementation"], observed["version"]) != ("cpython", "3.12.10"):
        raise ValueError("ORACLE_VERSION_MISMATCH")
    flags = bytearray()
    mappings = []
    for codepoint in range(0x110000):
        text = chr(codepoint)
        lower, folded = text.lower(), text.casefold()
        cased = text.islower() or text.isupper() or text.istitle()
        # Probe CPython's whole-string contextual lower implementation. Some
        # case-ignorable characters are also cased (for example U+0345).
        ignorable = (("AΣ" + text + "A").lower()[1] == "σ"
                     and ("AΣ" + text + " ").lower()[1] == "ς")
        predicates = (text.isspace(), text.isalpha(), text.isalnum(),
                      text.isdecimal(), text.isdigit(), text.isnumeric(),
                      text.isidentifier(), text.isprintable(), text.islower(),
                      text.isupper(), text.istitle(), cased, ignorable)
        bits = sum(int(value) << index for index, value in enumerate(predicates))
        flags.extend(struct.pack("<H", bits))
        if lower != text or folded != text:
            mappings.append((codepoint, tuple(map(ord, lower)), tuple(map(ord, folded))))
    header = json.dumps({"format": "graph-unicode", "version": 1,
                         "codepoints": 0x110000, "oracle": observed},
                        sort_keys=True, separators=(",", ":")).encode("ascii")
    data = bytearray(MAGIC + struct.pack("<I", len(header)) + header + flags)
    data.extend(struct.pack("<I", len(mappings)))
    for codepoint, lower, folded in mappings:
        data.extend(struct.pack("<IBB", codepoint, len(lower), len(folded)))
        for point in lower + folded:
            data.extend(struct.pack("<I", point))
    output.parent.mkdir(parents=True, exist_ok=True)
    if output.exists():
        raise ValueError("CANDIDATE_ALREADY_EXISTS")
    output.write_bytes(data)
    candidate = {
        "schema_version": 1,
        "input": {"path": RELATIVE_INPUT, "format": "graph-unicode", "version": 1,
                  "sha256": hashlib.sha256(data).hexdigest(), "size_bytes": len(data)},
        "generator": {"path": "tools/generate_unicode.py", "sha256": sha256(Path(__file__)),
                      "transitive_inputs": []},
        "oracle": observed,
    }
    report = output.with_suffix(".candidate.json")
    report.write_text(json.dumps(candidate, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"input": str(output), "candidate_manifest": str(report),
                      "sha256": candidate["input"]["sha256"], "size": len(data)}, sort_keys=True))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    generate(args.repo.resolve(), args.output.resolve())


if __name__ == "__main__":
    main()
