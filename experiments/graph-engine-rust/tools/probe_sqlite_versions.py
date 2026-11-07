"""Bind actual source and native SQLite versions before persistent-store work."""

import argparse
import json
import sqlite3
import subprocess
import sys
from pathlib import Path

sys.path.insert(0,str(Path(__file__).resolve().parent.parent / "parity"))
from bindings import capture


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("--rust",type=Path,required=True)
    parser.add_argument("--output",type=Path,required=True)
    args=parser.parse_args()
    completed=subprocess.run([str(args.rust.resolve())],input=b'{"op":"sqlite_version"}\n',capture_output=True,check=True)
    result=json.loads(completed.stdout)
    matches=result.get("ok") is True and result.get("result")==sqlite3.sqlite_version
    report=dict(python_sqlite=sqlite3.sqlite_version,native=result,matches=matches,bindings=capture(args.rust))
    args.output.write_text(json.dumps(report,indent=2)+"\n",encoding="ascii")
    print(json.dumps(dict(python_sqlite=sqlite3.sqlite_version,native=result,matches=matches)))
    return int(not matches)


if __name__=="__main__":raise SystemExit(main())
