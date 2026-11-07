"""Compare native pure policy rules and the complete frozen decimal mapping."""

import argparse
import json
import subprocess
import sys
import unicodedata
from pathlib import Path

from bindings import capture
from comparison import exact_equal


def cases(config, repo):
    requests = [dict(op="unicode_decimal_inventory")]
    key_values = [None, True, 1, 1.0, [], {}, "abc", ["a", "b"], [True, 1, "a"], [["a"]], {"a": 1}, [float("nan"), float("nan")]]
    for value in key_values:
        for required in key_values:
            for allowed in ([], ["a", "b"], None, [["a"]]):
                requests.append(dict(op="require_keys", value=value, required=required, allowed=allowed))
    values = [None, True, False, 0, 1.0, [], {}, "", "1", "1.2", "1.2.3.4",
              ".1.2", "1..2", "1.2.", "2.8.0", "-1.+02.3_000",
              " 1 . 2 . 3 ", "².1.0", "1_0.2.3", "_1.2.3", "1_.2.3",
              "\ud800.1.2", "1.0.0\x00"]
    values += [chr(point)+".1.2" for point in range(0x110000) if chr(point).isdecimal()]
    values += [chr(point)+"1.2.3"+chr(point) for point in range(0x110000) if chr(point).isspace()]
    values += ["9"*digits+".0.0" for digits in (4299, 4300, 4301)]
    requests += [dict(op="policy_version_tuple", value=value) for value in values]
    policies = [None, [], "", 0, {}]
    policies += [dict(compatible_engine=value) for value in (None, [], "", 0, {}, {"min": "0.0.0"}, {"min": "9.0.0"}, {"min": "2.0.0", "max_exclusive": "3.0.0"}, {"min": "-1.0.0", "max_exclusive": "0.0.0"}, {"min": " ٢.٧.٠ ", "max_exclusive": "３.０.０"})]
    for policy in policies:
        for value in (None, [], "bad", "-1.2.3", "2.8.0", "9.0.0"):
            requests.append(dict(op="policy_engine_compatible", value=value, policy=policy))
    defaults = config._default_profile_policy(repo)
    for node, template in defaults["node_templates"].items():
        expected = template["output_contract"]
        alternatives = [None, [], 1, {}, expected, dict(expected, extra=True)]
        for key in expected:
            removed = dict(expected)
            del removed[key]
            alternatives.append(removed)
            for value in (None, False, True, 0, 1, 1.0, [], "invalid"):
                changed = dict(expected)
                changed[key] = value
                alternatives.append(changed)
        requests += [dict(op="policy_output_contract", node=node, value=value) for value in alternatives]
    for effect in (None, [], {}, True, 3, "invalid", "filesystem_read", "filesystem_write", "command", "external_read", "external_write", "destructive", "publish", "deploy"):
        for target in (None, [], {}, True, 3, "repo:docs/", "runtime:artifacts/id/", "profile:software-engineering-graph/references/a", "git-diff-check", "unknown", "https://example.invalid/a"):
            requests.append(dict(op="policy_capability_shape", role="senior_engineer", commands=["git-diff-check"], value=dict(effect=effect, action="read", target_ref=target)))
    for value in (None, [], {}, "", "runtime:artifacts/id/a", "repo:src/a.py", "repo:src/../a.py", "repo:src/*.py", "repo:.codex/config", "repo:src/private.txt", "repo:src/"):
        for denied in ([], [""], ["**/private.txt"], [None], [3]):
            requests.append(dict(op="policy_repository_spelling", value=value, denied=denied))
    for value in (None, [], {}, "", "repo:src/a", "runtime:artifacts/id/", "runtime:artifacts/id/a", "runtime:artifacts/ID/a", "runtime:artifacts/id/*.txt", "runtime:elsewhere/a"):
        for roots in ([], ["artifacts/id/"], [None], [3], ["artifacts/"], ["artifacts/", None], [None, "artifacts/"]):
            requests.append(dict(op="policy_runtime_target", value=value, roots=roots, windows=sys.platform == "win32"))
    return requests


def oracle(request, config, repo):
    try:
        operation = request["op"]
        value = request.get("value")
        if operation == "unicode_decimal_inventory":
            result = [[point, unicodedata.decimal(chr(point))] for point in range(0x110000) if chr(point).isdecimal()]
        elif operation == "require_keys":
            result = config.require_keys(value, request["required"], request["allowed"], "f")
        elif operation == "policy_version_tuple":
            result = config.version_tuple(value)
        elif operation == "policy_engine_compatible":
            result = config.engine_version_compatible(value, request["policy"])
        elif operation == "policy_output_contract":
            result = config._validate_output_contract(request["node"], value)
        elif operation == "policy_capability_shape":
            result = config._validate_capability_shape(value, request["role"], set(request["commands"]))
        elif operation == "policy_repository_spelling":
            result = config._repository_target(repo, value, request["denied"])
        elif operation == "policy_runtime_target":
            result = config._runtime_target(value, request["roots"])
        else:
            raise AssertionError(operation)
        return dict(ok=True, result=json.loads(json.dumps(result)))
    except config.ContractError as error:
        return dict(ok=False, error=dict(kind="ContractError", field=error.field, code=error.code))
    except Exception as error:
        return dict(ok=False, error=dict(kind=type(error).__name__, message=str(error)))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--oracle", type=Path, required=True)
    parser.add_argument("--rust", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    sys.path.insert(0, str(args.oracle.resolve()))
    from graph_engine import config
    args.output.mkdir(parents=True, exist_ok=False)
    repo = args.output / "fixtures" / "repo"
    repo.mkdir(parents=True)
    requests = cases(config, repo)
    encoded = b"".join((json.dumps(request, sort_keys=True)+"\n").encode("ascii") for request in requests)
    completed = subprocess.run([str(args.rust.resolve())], input=encoded, capture_output=True, timeout=120)
    (args.output / "requests.ndjson").write_bytes(encoded)
    (args.output / "stdout.ndjson").write_bytes(completed.stdout)
    (args.output / "stderr.bin").write_bytes(completed.stderr)
    actual = [json.loads(line) for line in completed.stdout.splitlines()]
    expected, failures = [], []
    for index, request in enumerate(requests):
        response = oracle(request, config, repo)
        expected.append(response)
        if index >= len(actual) or not exact_equal(response, actual[index]):
            failures.append(dict(index=index, request=request, oracle=response, native=actual[index] if index < len(actual) else None))
    (args.output / "oracle.ndjson").write_text("".join(json.dumps(response, sort_keys=True)+"\n" for response in expected), encoding="ascii")
    report = dict(scope="pure policy rules; repository spelling on an inert ART repository, not filesystem adversarial proof", requests=len(requests), responses=len(actual), native_exit=completed.returncode, failures=failures,
                  python=sys.version, unicode_version=unicodedata.unidata_version,
                  decimal_codepoints=len(expected[0]["result"]), bindings=capture(args.rust))
    (args.output / "report.json").write_text(json.dumps(report, indent=2)+"\n", encoding="ascii")
    print(json.dumps(dict(requests=len(requests), responses=len(actual), native_exit=completed.returncode, failures=len(failures),
                          decimal_codepoints=report["decimal_codepoints"], first_failures=failures[:8]), ensure_ascii=True))
    return int(bool(failures) or completed.returncode != 0 or len(actual) != len(requests))


if __name__ == "__main__":
    raise SystemExit(main())
