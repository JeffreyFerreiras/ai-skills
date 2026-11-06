"""Compare private config/projection APIs with actual Windows path fixtures."""

import argparse
import copy
import json
import ntpath
import os
import posixpath
import subprocess
import sys
from pathlib import Path
from unittest.mock import patch

from bindings import capture
from comparison import exact_equal


def prepare(config, output):
    fixture = output / "fixtures"
    repo, runtime, skill = (fixture / name for name in ("repo","runtime","skill"))
    for directory in (repo / "docs",repo / "src",runtime / "artifacts/id",skill):
        directory.mkdir(parents=True,exist_ok=True)
    for path in (repo / "src/one.py",repo / "src/two.py",runtime / "artifacts/id/one.py"):
        path.write_text("fixture\n",encoding="utf-8")
    context = dict(repo=str(repo),runtime=str(runtime),skill=str(skill),environment={})
    base = config._default_profile_policy(repo)
    values = [None,True,False,0,1,1.0,"", "repo:src/", "runtime:artifacts/id/", "src/", [], {}, ["src/"], [None], [["src/"]]]
    requests=[]
    for windows in (False,True):
        for value in values:
            requests.append(dict(op="policy_location_key",value=value,windows=windows))
            requests.append(dict(op="policy_duplicates",value=value,windows=windows))
            for other in values:
                requests.append(dict(op="policy_under_root",value=value,roots=other,windows=windows))
                requests.append(dict(op="policy_roots_overlap",value=value,other=other,windows=windows))
    capabilities = [None, [], {}, {"effect":"filesystem_read"}, {"effect":"filesystem_read","action":"read"},
        dict(effect="filesystem_read",action="read",target_ref="repo:src/"),
        dict(effect="filesystem_write",action="edit",target_ref="repo:src/one.py"),
        dict(effect="command",action="run",target_ref="git-diff-check"),
        dict(effect="external_read",action="inspect",target_ref="andromeda"),
        dict(effect=[],action="read",target_ref="repo:src/"),
        dict(effect="filesystem_read",action=[],target_ref="repo:src/"),
        dict(effect="filesystem_read",action="read",target_ref=None),
        dict(effect="filesystem_read",action="read",target_ref="repo:src/",extra=True),
    ]
    for version in (None,1,2,3,True,1.0):
        policy=copy.deepcopy(base);policy["schema_version"]=version
        for role in (None,[],{},3,"unknown","senior_engineer","tech_lead","code_reviewer","impact_mapper"):
            for capability in capabilities:
                requests.append(dict(op="policy_role_allowed",value=policy,role=role,capability=capability,windows=True))
    for roots in values:
        for extra in values:
            policy=copy.deepcopy(base);policy["schema_version"]=2
            policy["implementation_roots"]=roots;policy["artifact_roots"]["repo"]=extra
            requests.append(dict(op="policy_role_allowed",value=policy,role="senior_engineer",capability=dict(effect="filesystem_write",action="edit",target_ref="repo:src/one.py"),windows=True))
    pairs=capabilities+[["ab"],[["a",1]],[[None,1]],[[[],1]], [1], ["a"], "a", "", {"effect":float("nan"),"action":None,"target_ref":None}]
    for candidate in pairs:
        for ceiling in pairs:
            for version in (None,1,True,1.0,2):
                requests.append(dict(op="projected_capability_within",value=candidate,ceiling=ceiling,version=version,windows=True))
    for target in ("repo:src/one.py","runtime:artifacts/id/one.py","profile:software-engineering-graph/references/a"):
        for denied in values:
            requests.append(dict(op="filesystem_capability_target",value=target,denied=denied))
    lease_values=values+[{},float("nan"),float("inf"),float("-inf"),-0.0,1.75,1e308,2**100," +١_２۳ ", "9"*4300,"9"*4301,"invalid"*100,"🐈"*400,"a'b", 'a"b', "\ud800", "\x00", "\n", "\u2028"]
    for value in lease_values:
        requests.append(dict(op="branch_lease_seconds",value=value))
        requests.append(dict(op="branch_lease_seconds",value={"limits":value}))
        requests.append(dict(op="branch_lease_seconds",value={"limits":{"branch_lease_seconds":value}}))
    source_cap=dict(effect="filesystem_read",action="read",target_ref="repo:src/one.py")
    for target in ("repo:src/one.py","repo:SRC/ONE.py","repo:src/missing.py","runtime:artifacts/id/one.py","runtime:artifacts/ID/ONE.py"):
        capability=dict(source_cap,target_ref=target)
        task=dict(authority=dict(capabilities=[capability]))
        for version in (None,1,True,2):
            for ceilings in ([capability],[dict(capability,target_ref=target.upper().replace("REPO:","repo:").replace("RUNTIME:","runtime:"))],[],[None]):
                policy=dict(denied_patterns=[],role_capabilities={"one":ceilings})
                requests.append(dict(context,op="policy_task_paths",task=task,policy=policy,version=version))
    for target,outer in [
        ("repo:src/one.py","repo:SRC/ONE.py"),("repo:src/one.py","repo:src/two.py"),
        ("repo:src/one.py","repo:src/missing.py"),("runtime:artifacts/id/one.py","runtime:ARTIFACTS/ID/ONE.py"),
        ("profile:software-engineering-graph/references/A","profile:software-engineering-graph/references/a"),
        (None,None),([],[]),(None,"repo:src/"),("repo:src/one.py",None),("missing","repo:src/one.py"),
    ]:
        requests.append(dict(context,op="policy_case_identity",value=target,outer=outer))
    features={}
    if os.name=="nt":
        destination=fixture/"junction-target";destination.mkdir();(destination/"one.py").write_text("fixture",encoding="utf-8")
        link=repo/"junction"
        argv=["cmd.exe","/d","/c","mklink","/J",str(link),str(destination)]
        completed=subprocess.run(argv,capture_output=True)
        features["junction"]=dict(argv=argv,exit=completed.returncode,stdout=completed.stdout.decode("utf-8",errors="replace"),stderr=completed.stderr.decode("utf-8",errors="replace"))
        if completed.returncode==0:
            capability=dict(source_cap,target_ref="repo:junction/one.py")
            requests.append(dict(context,op="policy_task_paths",task=dict(authority=dict(capabilities=[capability])),policy=dict(denied_patterns=[],role_capabilities={"one":[capability]}),version=1))
            requests.append(dict(context,op="policy_case_identity",value="repo:junction/one.py",outer="repo:JUNCTION/ONE.py"))
        case=repo/"case";case.mkdir()
        argv=["fsutil.exe","file","setCaseSensitiveInfo",str(case),"enable"]
        completed=subprocess.run(argv,capture_output=True)
        features["case_sensitive_directory"]=dict(argv=argv,exit=completed.returncode,stdout=completed.stdout.decode("utf-8",errors="replace"),stderr=completed.stderr.decode("utf-8",errors="replace"))
        if completed.returncode==0:
            first,second=case/"Feature.py",case/"feature.py"
            first.write_text("first",encoding="utf-8");second.write_text("second",encoding="utf-8")
            features["case_sensitive_directory"]["distinct_inode"]=first.stat().st_ino!=second.stat().st_ino
            if first.stat().st_ino!=second.stat().st_ino:
                capability=dict(source_cap,target_ref="repo:case/Feature.py")
                other=dict(capability,target_ref="repo:case/feature.py")
                requests.append(dict(context,op="policy_task_paths",task=dict(authority=dict(capabilities=[capability])),policy=dict(denied_patterns=[],role_capabilities={"one":[capability],"two":[other]}),version=1))
                requests.append(dict(context,op="policy_case_identity",value="repo:case/Feature.py",outer="repo:case/feature.py"))
    return requests,features


def oracle(request,config,contracts):
    try:
        operation=request["op"];value=request.get("value")
        if operation in ("policy_task_paths","policy_case_identity"):
            repo=Path(request["repo"]);runtime=Path(request["runtime"]) if request.get("runtime") is not None else None
            if operation=="policy_task_paths":
                result=config.validate_task_capability_paths(repo,request["task"],request["policy"],runtime,request["version"])
            else:
                result=config._prove_capability_case_identity(repo,runtime,value,request["outer"])
        else:
            with patch("graph_engine.config.os.path.normcase",ntpath.normcase if request.get("windows",True) else posixpath.normcase),patch("graph_engine.config.os.sep","\\" if request.get("windows",True) else "/"):
                if operation=="policy_location_key":result=config._location_key(value)
                elif operation=="policy_under_root":result=config._under_configured_root(value,request["roots"])
                elif operation=="policy_roots_overlap":result=config._roots_overlap(value,request["other"])
                elif operation=="policy_duplicates":result=config._has_location_duplicates(value)
                elif operation=="policy_role_allowed":result=config.role_capability_allowed(value,request["role"],request["capability"])
                elif operation=="projected_capability_within":result=contracts.projected_capability_within(value,request["ceiling"],request["version"])
                elif operation=="filesystem_capability_target":result=contracts.filesystem_capability_target(value,request["denied"])
                elif operation=="branch_lease_seconds":result=config.branch_lease_seconds(value)
                else:raise AssertionError(operation)
        return dict(ok=True,result=json.loads(json.dumps(result)))
    except config.ContractError as error:return dict(ok=False,error=dict(kind="ContractError",field=error.field,code=error.code))
    except Exception as error:return dict(ok=False,error=dict(kind=type(error).__name__,message=str(error)))


def main():
    parser=argparse.ArgumentParser();parser.add_argument("--oracle",type=Path,required=True);parser.add_argument("--rust",type=Path,required=True);parser.add_argument("--output",type=Path,required=True);args=parser.parse_args()
    sys.path.insert(0,str(args.oracle.resolve()))
    from graph_engine import config,contracts
    args.output.mkdir(parents=True,exist_ok=False)
    requests,features=prepare(config,args.output)
    encoded=b"".join((json.dumps(request,sort_keys=True)+"\n").encode("ascii") for request in requests)
    completed=subprocess.run([str(args.rust.resolve())],input=encoded,capture_output=True,timeout=180)
    (args.output/"requests.ndjson").write_bytes(encoded);(args.output/"stdout.ndjson").write_bytes(completed.stdout);(args.output/"stderr.bin").write_bytes(completed.stderr)
    actual=[json.loads(line) for line in completed.stdout.splitlines()]
    failures=[];expected=[]
    for index,request in enumerate(requests):
        response=oracle(request,config,contracts);expected.append(response)
        if index>=len(actual) or not exact_equal(response,actual[index]):failures.append(dict(index=index,request=request,oracle=response,native=actual[index] if index<len(actual) else None))
    (args.output/"oracle.ndjson").write_text("".join(json.dumps(response,sort_keys=True)+"\n" for response in expected),encoding="ascii")
    report=dict(scope="private API equivalence and native Windows path fixtures; additional cases give no original-identity credit",requests=len(requests),responses=len(actual),native_exit=completed.returncode,failures=failures,features=features,contract_is_value_error=issubclass(config.ContractError,ValueError),bindings=capture(args.rust))
    (args.output/"report.json").write_text(json.dumps(report,indent=2)+"\n",encoding="ascii")
    first=[dict(index=item["index"],op=item["request"]["op"],oracle_error=item["oracle"].get("error"),native_error=item["native"].get("error") if item["native"] else "MISSING_RESPONSE") for item in failures[:8]]
    print(json.dumps(dict(requests=len(requests),responses=len(actual),native_exit=completed.returncode,failures=len(failures),features=features,first_failures=first)))
    return int(bool(failures) or completed.returncode!=0 or len(actual)!=len(requests))


if __name__=="__main__":raise SystemExit(main())
