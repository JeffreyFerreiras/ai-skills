"""Compare raw parser-only process streams on generated/native executables."""

import base64
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


def encoding_corpus(output, native, script, previous_path=None):
    previous = None
    if previous_path is not None:
        previous = json.loads(previous_path.read_bytes())
        if previous["oracle_script_sha256"] != hashlib.sha256(script.read_bytes()).hexdigest():
            raise ValueError("ORACLE_SOURCE_CHANGED")
    modes = [("inherited", {}), ("python_utf8", {"PYTHONUTF8": "1"}),
             *[(encoding, {"PYTHONIOENCODING": encoding}) for encoding in ("ascii", "latin1", "utf-8", "utf-16")]]
    arguments = [["--help"], ["status", "--help"], ["status"], *[[text] for text in ("é", "€", "Σ", "漢", "😀")]]
    records = []
    for mode, overrides in modes:
        environment = {**os.environ, **overrides}
        for argv in arguments:
            record = {"mode": mode, "argv": argv, "environment": {key: environment.get(key) for key in ("PYTHONUTF8", "PYTHONIOENCODING", "COLUMNS")}}
            for name, command in (("oracle", [sys.executable, "-B", str(script)]), ("native", [str(native)])):
                if name == "oracle" and previous is not None:
                    prior = next(item for item in previous["records"] if item["mode"] == mode and item["argv"] == argv)
                    if prior["environment"] != record["environment"]:
                        raise ValueError("ORACLE_ENVIRONMENT_CHANGED")
                    record[name] = prior["oracle"]
                    continue
                try:
                    completed = subprocess.run(command + argv, env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=15, check=False)
                    record[name] = {"exit_code": completed.returncode, "stdout_base64": base64.b64encode(completed.stdout).decode("ascii"), "stderr_base64": base64.b64encode(completed.stderr).decode("ascii"), "timeout": False}
                except subprocess.TimeoutExpired as error:
                    record[name] = {"exit_code": None, "stdout_base64": base64.b64encode(error.stdout or b"").decode("ascii"), "stderr_base64": base64.b64encode(error.stderr or b"").decode("ascii"), "timeout": True}
            record["matches"] = record["oracle"] == record["native"]
            records.append(record)
    result = {"scope": "8 parser-only argv x6 child-env modes,48oracle+48native children; no helper operation", "oracle_executable": sys.executable, "oracle_script_sha256": hashlib.sha256(script.read_bytes()).hexdigest(), "native_sha256": hashlib.sha256(native.read_bytes()).hexdigest(), "records": records, "differences": sum(not record["matches"] for record in records)}
    if previous is not None:
        result["scope"] = "48native parser-only children compared with immutable48oracle records; no new oracle child"
        result["oracle_reused_from"] = {"path": str(previous_path), "sha256": hashlib.sha256(previous_path.read_bytes()).hexdigest()}
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes((json.dumps(result, ensure_ascii=True, sort_keys=True, indent=2) + "\n").encode("ascii"))
    print(json.dumps({"output": str(output), "records": len(records), "differences": result["differences"], "failed_cases": [{"mode": record["mode"], "argv": record["argv"], "source_stderr_tail_hex": base64.b64decode(record["oracle"]["stderr_base64"])[-160:].hex(), "native_stderr_tail_hex": base64.b64decode(record["native"]["stderr_base64"])[-160:].hex()} for record in records if not record["matches"]]}, ensure_ascii=True, sort_keys=True))


def main():
    root = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve(strict=True)
    output = Path(sys.argv[1]).resolve()
    output.relative_to(root)
    native = Path(sys.argv[2]).resolve(strict=True)
    native.relative_to(root)
    script = Path(__file__).resolve().parents[3] / "skills/software-engineering-graph/scripts/helper-register.py"
    if len(sys.argv) >= 4 and sys.argv[3] == "--encoding-corpus":
        previous_path = Path(sys.argv[4]).resolve(strict=True) if len(sys.argv) == 5 else None
        if previous_path is not None: previous_path.relative_to(root)
        encoding_corpus(output, native, script, previous_path)
        return
    previous = None
    if len(sys.argv) == 4:
        previous_path = Path(sys.argv[3]).resolve(strict=True)
        previous_path.relative_to(root)
        previous = json.loads(previous_path.read_bytes())
        if previous["oracle_script_sha256"] != hashlib.sha256(script.read_bytes()).hexdigest():
            raise ValueError("ORACLE_SOURCE_CHANGED")
    records = []
    for argv in (["--help"], ["status", "--help"], ["status"]):
        record = {"argv": argv}
        for name, command in (("oracle", [sys.executable, "-B", str(script)]), ("native", [str(native)])):
            if name == "oracle" and previous is not None:
                record[name] = next(item["oracle"] for item in previous["records"] if item["argv"] == argv)
                continue
            completed = subprocess.run(command + argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=15, check=False)
            record[name] = {"exit_code": completed.returncode, "stdout_base64": base64.b64encode(completed.stdout).decode("ascii"), "stderr_base64": base64.b64encode(completed.stderr).decode("ascii"), "stdout_hex": completed.stdout.hex(), "stderr_hex": completed.stderr.hex()}
        record["matches"] = record["oracle"] == record["native"]
        records.append(record)
    result = {"scope": "three parser-only argv exit before register construction; raw bytes", "parent_streams": {"stdout_encoding": sys.stdout.encoding, "stdout_errors": sys.stdout.errors, "stderr_encoding": sys.stderr.encoding, "stderr_errors": sys.stderr.errors, "os_linesep_hex": os.linesep.encode().hex()}, "COLUMNS": os.environ.get("COLUMNS"), "oracle_executable": sys.executable, "oracle_script_sha256": hashlib.sha256(script.read_bytes()).hexdigest(), "native_sha256": hashlib.sha256(native.read_bytes()).hexdigest(), "records": records, "differences": sum(not record["matches"] for record in records)}
    if previous is not None:
        result["oracle_reused_from"] = {"path": str(previous_path), "sha256": hashlib.sha256(previous_path.read_bytes()).hexdigest()}
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes((json.dumps(result, sort_keys=True, indent=2) + "\n").encode("ascii"))
    tails = [{"argv": record["argv"], **{name: {"exit_code": record[name]["exit_code"], "stdout_tail_hex": record[name]["stdout_hex"][-64:], "stderr_tail_hex": record[name]["stderr_hex"][-64:]} for name in ("oracle", "native")}} for record in records]
    print(json.dumps({"output": str(output), "records": len(records), "differences": result["differences"], "parent_streams": result["parent_streams"], "tails": tails}, sort_keys=True))


if __name__ == "__main__":
    main()
