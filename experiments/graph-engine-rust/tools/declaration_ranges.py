"""Print declaration metadata without source bodies or docstrings."""

import ast
import hashlib
import json
from pathlib import Path
import sys


def main():
    path = Path(sys.argv[1])
    data = path.read_bytes()
    tree = ast.parse(data, filename=str(path))
    declarations = []
    for node in ast.walk(tree):
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            declarations.append(dict(name=node.name, kind=type(node).__name__, start=node.lineno, end=node.end_lineno))
    print(json.dumps(dict(path=str(path), sha256=hashlib.sha256(data).hexdigest(), lines=len(data.splitlines()), declarations=sorted(declarations, key=lambda item:item["start"]))))


if __name__ == "__main__":
    main()
