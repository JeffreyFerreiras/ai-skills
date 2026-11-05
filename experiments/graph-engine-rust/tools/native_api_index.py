"""Record native public declaration names without source bodies."""

import hashlib
import json
import os
from pathlib import Path
import re
import tomllib


def main():
    package = Path(__file__).resolve().parent.parent
    artifact = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve(strict=True)
    declaration = re.compile(r"^\s*pub(?:\([^)]*\))?\s+(?:(?:async|unsafe|const)\s+)*(fn|struct|type|trait|enum)\s+(\w+)")
    reexport = re.compile(r"^\s*pub\s+use\s+(.+?);\s*$")
    records = []
    hashes = {}
    for path in sorted((package / "src").rglob("*.rs")):
        data = path.read_bytes()
        name = path.relative_to(package).as_posix()
        hashes[name] = hashlib.sha256(data).hexdigest()
        for number, line in enumerate(data.decode("utf-8").splitlines(), 1):
            matched = declaration.match(line)
            if matched:
                records.append(dict(file=name, line=number, kind=matched[1], name=matched[2]))
            else:
                matched = reexport.match(line)
                if matched:
                    names = re.findall(r"[A-Za-z_]\w*", matched[1])
                    records.append(dict(file=name, line=number, kind="reexport", names=names))
    if len(records) > 2000:
        raise ValueError("PUBLIC_API_RECORD_LIMIT")
    manifest = tomllib.loads((package / "Cargo.toml").read_text(encoding="utf-8"))
    report = dict(scope="public declaration metadata only; no bodies or behavioral proof", records=records,
                  source_sha256=hashes, lib_name=manifest.get("lib", {}).get("name"),
                  cargo_manifest_sha256=hashlib.sha256((package / "Cargo.toml").read_bytes()).hexdigest())
    output = artifact / "native-api-index-01.json"
    with output.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(report, stream, indent=2)
        stream.write("\n")
    print(json.dumps(dict(path=str(output), records=len(records), files=len(hashes), lib_name=report["lib_name"],
                          sha256=hashlib.sha256(output.read_bytes()).hexdigest(),
                          relevant=[record for record in records if record["file"] in {"src/compat/json.rs", "src/compat/mod.rs"}])) )


if __name__ == "__main__":
    main()
