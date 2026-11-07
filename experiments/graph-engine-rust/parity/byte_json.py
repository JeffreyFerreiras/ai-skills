"""Compare Python json.loads value/byte codec ingress used by stored SQL responses."""

import argparse
import json
import subprocess
import sys
from pathlib import Path

from bindings import capture
from comparison import exact_equal


def cases():
    values=[None,True,False,0,1.5,[],{},"","{}","[]","null","NaN","Infinity","-0.0","\ufeff{}", "\n invalid", '"\ud800"']
    requests=[dict(op="loads_value",parameter=dict(type="value",value=value)) for value in values]
    texts=["","{}","[]","null","NaN","Infinity","-0.0",'{"value":"🐈"}','"\ud800"','"\udc80"',"\ufeff{}","\ninvalid",'{"a":1,}']
    for text in texts:
        for encoding in ("utf-8","utf-8-sig","utf-16","utf-16-le","utf-16-be","utf-32","utf-32-le","utf-32-be"):
            raw=text.encode(encoding,errors="surrogatepass")
            for kind in ("bytes","bytearray"):
                requests.append(dict(op="loads_value",parameter=dict(type=kind,hex=raw.hex())))
    malformed=[bytes.fromhex(value) for value in ("ff","80","c0","c2","e0","e180","e18041","e08080","f09080","f4908080","f5808080",
               "efbbbfff","fffe00","feff00","fffe000000","0000feff00","0000007b00","7b00000000","00001100","00110000","eda080")]
    for raw in malformed:
        requests.append(dict(op="loads_value",parameter=dict(type="bytes",hex=raw.hex())))
    for depth in (900,2997,2998,5000):
        raw=("["*depth+"0"+"]"*depth).encode("ascii")
        requests.append(dict(op="loads_value",parameter=dict(type="bytes",hex=raw.hex())))
    integers=[None,True,False,0,1.75,-0.0,1e308,float("nan"),float("inf"),float("-inf"),[],{},2**100," +١_２۳ ","\u0085-০০৭\u3000","1__2","a'b",'a"b',"\ud800","🐈"*400,"0"*5000,"9"*4300,"9"*4301]
    requests += [dict(op="python_int",parameter=dict(type="value",value=value)) for value in integers]
    for raw in (b"",b" +1_23 ",b"\x851",b"\xff",b"1__2",b"a'b",b'9'*4300,b'9'*4301):
        requests.append(dict(op="python_int",parameter=dict(type="bytes",hex=raw.hex())))
    return requests


def oracle(request):
    parameter=request["parameter"]
    value=parameter.get("value")
    if parameter["type"] in ("bytes","bytearray"):
        value=bytes.fromhex(parameter["hex"])
        if parameter["type"]=="bytearray":value=bytearray(value)
    try:
        if request["op"]=="python_int":return dict(ok=True,result=int(value))
        return dict(ok=True,result=json.dumps(json.loads(value),sort_keys=True,separators=(",",":")))
    except Exception as error:return dict(ok=False,error=dict(kind=type(error).__name__,message=str(error)))


def main():
    parser=argparse.ArgumentParser();parser.add_argument("--rust",type=Path,required=True);parser.add_argument("--output",type=Path,required=True);args=parser.parse_args()
    args.output.mkdir(parents=True,exist_ok=False);requests=cases()
    encoded=b"".join((json.dumps(request,sort_keys=True)+"\n").encode("ascii") for request in requests)
    completed=subprocess.run([str(args.rust.resolve())],input=encoded,capture_output=True,timeout=120)
    (args.output/"requests.ndjson").write_bytes(encoded);(args.output/"stdout.ndjson").write_bytes(completed.stdout);(args.output/"stderr.bin").write_bytes(completed.stderr)
    (args.output/"native-process.json").write_text(json.dumps(dict(native_exit=completed.returncode,requests=len(requests),responses=len(completed.stdout.splitlines()),bindings=capture(args.rust)),indent=2)+"\n",encoding="ascii")
    actual=[json.loads(line) for line in completed.stdout.splitlines()];expected=[];failures=[]
    for index,request in enumerate(requests):
        response=oracle(request);expected.append(response)
        if index>=len(actual) or not exact_equal(response,actual[index]):failures.append(dict(index=index,request=request,oracle=response,native=actual[index] if index<len(actual) else None))
    (args.output/"oracle.ndjson").write_text("".join(json.dumps(response,sort_keys=True)+"\n" for response in expected),encoding="ascii")
    report=dict(scope="per-call stored-response json.loads ingress, including bytes/bytearray codecs and errors",python=sys.version,requests=len(requests),responses=len(actual),native_exit=completed.returncode,failures=failures,bindings=capture(args.rust))
    (args.output/"report.json").write_text(json.dumps(report,indent=2)+"\n",encoding="ascii")
    first=[dict(index=item["index"],parameter_type=item["request"]["parameter"]["type"],hex=item["request"]["parameter"].get("hex","")[:80],oracle_error=item["oracle"].get("error"),native_error=item["native"].get("error") if item["native"] else "MISSING_RESPONSE") for item in failures[:8]]
    print(json.dumps(dict(requests=len(requests),responses=len(actual),native_exit=completed.returncode,failures=len(failures),first_failures=first)))
    return int(bool(failures) or completed.returncode!=0 or len(actual)!=len(requests))


if __name__=="__main__":raise SystemExit(main())
