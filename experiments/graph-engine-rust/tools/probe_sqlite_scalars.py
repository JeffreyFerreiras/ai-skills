"""Probe selected Python SQLite row and binding behavior on isolated ART data."""

import argparse
import json
import math
import sqlite3
import sys
from pathlib import Path


def describe(value):
    if isinstance(value,bytes):return dict(type="bytes",hex=value.hex())
    if isinstance(value,dict):return dict(type="dict",items=[[key,describe(item)] for key,item in value.items()])
    return dict(type=type(value).__name__,value=value)


def main():
    parser=argparse.ArgumentParser();parser.add_argument("--output",type=Path,required=True);args=parser.parse_args()
    args.output.mkdir(parents=True,exist_ok=False)
    connection=sqlite3.connect(args.output/"probe.sqlite3")
    connection.row_factory=sqlite3.Row
    cases=[("SELECT x'0001ff' AS value",[]),("SELECT CAST(x'ff' AS TEXT) AS value",[]),
           ("SELECT CAST(x'eda080' AS TEXT) AS value",[]),("SELECT 1 AS a,2 AS A",[]),
           ('SELECT 1 AS "Ä",2 AS "ä"',[]),("SELECT 1 AS same,2 AS same",[])]
    for value in [None,True,False,0,2**63-1,2**63,-2**63-1,-0.0,1.25,float("nan"),float("inf"),float("-inf"),"",b"\xff", "\ud800", "before\ud800after"]:
        cases.append(("SELECT ? AS value",[value]))
    records=[]
    for sql,parameters in cases:
        record=dict(sql=sql,parameters=[describe(value) for value in parameters])
        try:
            row=connection.execute(sql,parameters).fetchone()
            result={key:row[key] for key in row.keys()}
            record.update(result=describe(result),keys=row.keys())
            try:record["json"]=json.dumps(result,sort_keys=True,separators=(",",":"))
            except Exception as error:record["json_error"]=dict(kind=type(error).__name__,message=str(error))
        except Exception as error:record["error"]=dict(kind=type(error).__name__,message=str(error))
        records.append(record)
    connection.close()
    report=dict(python=sys.version,sqlite=sqlite3.sqlite_version,records=records)
    (args.output/"report.json").write_text(json.dumps(report,indent=2,ensure_ascii=True)+"\n",encoding="ascii")
    print(json.dumps(report,ensure_ascii=True))


if __name__=="__main__":main()
