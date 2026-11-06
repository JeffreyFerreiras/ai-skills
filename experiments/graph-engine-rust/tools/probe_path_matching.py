"""Record targeted runtime path semantics without reading stdlib implementation."""

import argparse
import json
import platform
import sys
from pathlib import Path, PurePosixPath


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    patterns = ["*", "**", "?", "??", "[!a-z]", "[!z-a]", "[!]]", "[!/]", "[/]", "[\n]", "[!\n]", "/", "//", "/*", "//*", "**/*"]
    observations = []
    for text in ["", ".", "/", "//", "///", "/a", "//a", "a", "a/b", "a\nb", "\n"]:
        path = PurePosixPath(text)
        observations.append(dict(text=text, normalized=str(path), parts=path.parts, lines=path._lines,
                                 anchor=path.anchor,
                                 matches={pattern: path.match(pattern) for pattern in patterns}))
    report = dict(python=sys.version, implementation=platform.python_implementation(),
                  observations=observations)
    args.output.write_text(json.dumps(report, indent=2, ensure_ascii=True)+"\n", encoding="ascii")
    print(json.dumps(report, ensure_ascii=True))


if __name__ == "__main__":
    main()
