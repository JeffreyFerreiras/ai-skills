"""Restore the exact frozen Unicode artifact, without claiming new oracle provenance.

The manifest's oracle header describes the historical artifact, not the Python
executing this restoration. Only byte-for-byte equality to its frozen SHA-256
permits publication. A different interpreter's Unicode behavior fails closed.
"""

import argparse
import hashlib
import json
import os
import struct
import tempfile
from pathlib import Path

MANIFEST_SHA256 = "29b2cdb077bc22a61a9cfc1180062fc5fd4336b5c8169d80a5a44383c4ef703f"
INPUT_PATH = "compat/unicode/cpython-3.12.10/unicode-tables.bin"
DEFAULT_MANIFEST = Path(__file__).resolve().parents[1] / "compat/unicode-input-manifest.json"


def load_manifest(path):
    raw = path.read_bytes()
    # Git checkouts may normalize newlines. The frozen native verifier hashes
    # the original CRLF representation; accept only that exact content here.
    canonical = raw.replace(b"\r\n", b"\n").replace(b"\n", b"\r\n")
    if hashlib.sha256(canonical).hexdigest() != MANIFEST_SHA256:
        raise ValueError("COMPAT_MANIFEST_DIGEST_MISMATCH")
    return json.loads(raw)


def reconstruct(manifest):
    flags = bytearray()
    mappings = []
    for codepoint in range(0x110000):
        text = chr(codepoint)
        lower, folded = text.lower(), text.casefold()
        cased = text.islower() or text.isupper() or text.istitle()
        ignorable = (("AΣ" + text + "A").lower()[1] == "σ"
                     and ("AΣ" + text + " ").lower()[1] == "ς")
        predicates = (text.isspace(), text.isalpha(), text.isalnum(),
                      text.isdecimal(), text.isdigit(), text.isnumeric(),
                      text.isidentifier(), text.isprintable(), text.islower(),
                      text.isupper(), text.istitle(), cased, ignorable)
        flags.extend(struct.pack("<H", sum(int(value) << index
                                         for index, value in enumerate(predicates))))
        if lower != text or folded != text:
            mappings.append((codepoint, tuple(map(ord, lower)), tuple(map(ord, folded))))
    # Preserve the historical artifact header; do not describe this interpreter
    # as the original oracle. The complete frozen digest authenticates the result.
    header = json.dumps({"format": "graph-unicode", "version": 1,
                         "codepoints": 0x110000, "oracle": manifest["oracle"]},
                        sort_keys=True, separators=(",", ":")).encode("ascii")
    data = bytearray(b"GUNICOD1" + struct.pack("<I", len(header)) + header + flags)
    data.extend(struct.pack("<I", len(mappings)))
    for codepoint, lower, folded in mappings:
        data.extend(struct.pack("<IBB", codepoint, len(lower), len(folded)))
        for point in lower + folded:
            data.extend(struct.pack("<I", point))
    return data


def verify(data, manifest):
    if (len(data) != manifest["input"]["size_bytes"]
            or hashlib.sha256(data).hexdigest() != manifest["input"]["sha256"]):
        raise ValueError("COMPAT_INPUT_DIGEST_MISMATCH: use an interpreter with compatible Unicode semantics")


def restore(manifest_path, artifact_root):
    manifest = load_manifest(manifest_path)
    if not artifact_root.is_absolute():
        raise ValueError("COMPAT_ARTIFACT_ROOT_ABSOLUTE_REQUIRED")
    output = artifact_root / INPUT_PATH
    if any(parent.is_symlink() for parent in output.parents) or output.is_symlink():
        raise ValueError("COMPAT_LINK_FORBIDDEN")
    if output.exists():
        verify(output.read_bytes(), manifest)
        return output
    data = reconstruct(manifest)
    verify(data, manifest)
    output.parent.mkdir(parents=True, exist_ok=True)
    # Publish a fully written file without replacing any existing artifact.
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(dir=output.parent, delete=False) as stream:
            temporary = Path(stream.name)
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        try:
            os.link(temporary, output)
        except FileExistsError:
            verify(output.read_bytes(), manifest)
    finally:
        if temporary is not None:
            temporary.unlink()
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifact-root", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    args = parser.parse_args()
    output = restore(args.manifest, args.artifact_root)
    print(json.dumps({"input": str(output), "sha256": hashlib.sha256(output.read_bytes()).hexdigest(),
                      "method": "byte-for-byte restoration of frozen artifact"}, sort_keys=True))


if __name__ == "__main__":
    main()
