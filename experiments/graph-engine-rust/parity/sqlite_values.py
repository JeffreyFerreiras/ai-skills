"""Check owned SQL scalars and byte JSON errors against selected CPython."""

import argparse
import json
import sqlite3
import subprocess
import sys
from pathlib import Path

from bindings import capture
from comparison import exact_equal


def describe(value):
    if isinstance(value,bytes):return dict(type="bytes",hex=value.hex())
    if isinstance(value,dict):return dict(type="dict",items=[[key,describe(item)] for key,item in value.items()])
    return dict(type=type(value).__name__,value=value)


def cases():
    requests=[]
    queries=["SELECT x'0001ff' AS value","SELECT CAST(x'ff' AS TEXT) AS value","SELECT CAST(x'eda080' AS TEXT) AS value",
             "SELECT CAST(x'6100ff' AS TEXT) AS value","SELECT 1 AS a,2 AS A",'SELECT 1 AS "Ä",2 AS "ä"',
             "SELECT 1 AS same,2 AS same","SELECT 1 AS same,CAST(x'ff' AS TEXT) AS same",
             "SELECT CAST(x'ff' AS TEXT) AS a,2 AS A"]
    requests += [dict(op="sqlite_rows",sql=sql,parameters=[]) for sql in queries]
    values=[None,True,False,0,2**63-1,2**63,-2**63-1,-0.0,1.25,float("nan"),float("inf"),float("-inf"),"",
            "\ud800","before\ud800after","\ud800\ud801",[],{},[1],"a\x00b"]
    for value in values:
        requests.append(dict(op="sqlite_rows",sql="SELECT ? AS value",parameters=[dict(type="value",value=value)]))
    requests.append(dict(op="sqlite_rows",sql="SELECT ? AS value",parameters=[dict(type="bytes",hex="00ff")]))
    for sql in ("SELECT ? AS value","SELECT ? AS value,? AS other","SELECT * FROM no_such_fixture_table"):
        for parameters in ([],[dict(type="value",value=2**63)],[dict(type="value",value=1),dict(type="value",value=2),dict(type="value",value=3)]):
            requests.append(dict(op="sqlite_rows",sql=sql,parameters=parameters))
    requests += [dict(op="bytes_profile",depth=depth,container=container) for container in ("list","object") for depth in (0,1,900,*range(2990,3000),5000)]
    return requests


def oracle(request,connection):
    try:
        if request["op"]=="bytes_profile":
            value=b"\x00\xff"
            for _ in range(request["depth"]):value={"value":value} if request.get("container")=="object" else [value]
            result=json.dumps(value,sort_keys=True,separators=(",",":"),ensure_ascii=False).encode("utf-8").decode("utf-8")
        else:
            parameters=[bytes.fromhex(parameter["hex"]) if parameter["type"]=="bytes" else parameter["value"] for parameter in request["parameters"]]
            rows=connection.execute(request["sql"],parameters).fetchall()
            result=[]
            for row in rows:
                value={key:row[key] for key in row.keys()}
                record=dict(result=describe(value))
                try:record["json"]=json.dumps(value,sort_keys=True,separators=(",",":"))
                except Exception as error:record["json_error"]=dict(kind=type(error).__name__,message=str(error))
                result.append(record)
        return dict(ok=True,result=result)
    except Exception as error:return dict(ok=False,error=dict(kind=type(error).__name__,message=str(error)))


def main():
    parser=argparse.ArgumentParser();parser.add_argument("--rust",type=Path,required=True);parser.add_argument("--output",type=Path,required=True);args=parser.parse_args()
    args.output.mkdir(parents=True,exist_ok=False)
    requests=cases();encoded=b"".join((json.dumps(request,sort_keys=True)+"\n").encode("ascii") for request in requests)
    completed=subprocess.run([str(args.rust.resolve())],input=encoded,capture_output=True,timeout=120)
    (args.output/"requests.ndjson").write_bytes(encoded);(args.output/"stdout.ndjson").write_bytes(completed.stdout);(args.output/"stderr.bin").write_bytes(completed.stderr)
    actual=[json.loads(line) for line in completed.stdout.splitlines()]
    connection=sqlite3.connect(args.output/"oracle.sqlite3");connection.row_factory=sqlite3.Row
    failures=[];expected=[]
    for index,request in enumerate(requests):
        response=oracle(request,connection);expected.append(response)
        if index>=len(actual) or not exact_equal(response,actual[index]):failures.append(dict(index=index,request=request,oracle=response,native=actual[index] if index<len(actual) else None))
    connection.close()
    (args.output/"oracle.ndjson").write_text("".join(json.dumps(response,sort_keys=True)+"\n" for response in expected),encoding="ascii")
    report=dict(scope="actual owned SQL bytes/row/binding/private serialization equivalence, no original-identity credit",python=sys.version,sqlite=sqlite3.sqlite_version,requests=len(requests),responses=len(actual),native_exit=completed.returncode,failures=failures,bindings=capture(args.rust))
    (args.output/"report.json").write_text(json.dumps(report,indent=2)+"\n",encoding="ascii")
    print(json.dumps(dict(requests=len(requests),responses=len(actual),native_exit=completed.returncode,failures=len(failures),first_failures=failures[:8])))
    return int(bool(failures) or completed.returncode!=0 or len(actual)!=len(requests))


if __name__=="__main__":raise SystemExit(main())
