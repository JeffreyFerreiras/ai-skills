"""Inspect only the two retained synthetic fixture roots from registry-05."""

import datetime
import hashlib
import json
import os
import sys
from pathlib import Path


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    artifact = Path(os.environ["GRAPH_RUST_RUN_ARTIFACT_ROOT"]).resolve(strict=True)
    matched = sys.argv[1:] == ["--matched"]
    names = ("helper-register-43472-0", "helper-register-44292-0") if matched else ("helper-register-16336-0", "helper-register-23724-0")
    records = []
    for name in names:
        root = artifact / "fixtures" / name
        record = {"root": str(root), "exists": root.exists()}
        if not root.exists():
            records.append(record)
            continue
        root.resolve(strict=True).relative_to(artifact / "fixtures")
        record["created_utc"] = datetime.datetime.fromtimestamp(root.stat().st_ctime, datetime.timezone.utc).isoformat()
        matches = list((root / "host-state/helper-registers").glob("*/register.json"))
        record["register_candidates"] = [str(path.relative_to(root)) for path in matches]
        if len(matches) != 1:
            record["diagnostic"] = "absent or ambiguous exact register candidate; no broadened lookup"
            records.append(record)
            continue
        path = matches[0]
        for inspected in (path, root / "repo/docs/helper-plan.json", root / "repo/docs/helper-allowance.json", root / "repo/docs/host-observation.json"):
            relative = inspected.relative_to(root)
            component = root
            for part in relative.parts:
                if component.lstat().st_file_attributes & 0x400:
                    raise ValueError("REPARSE_PATH_NOT_INSPECTED")
                component = component / part
            if component.lstat().st_file_attributes & 0x400:
                raise ValueError("REPARSE_FILE_NOT_INSPECTED")
        record["register_created_utc"] = datetime.datetime.fromtimestamp(path.stat().st_ctime, datetime.timezone.utc).isoformat()
        stored = json.loads(path.read_bytes())
        plan_path = root / "repo/docs/helper-plan.json"
        allowance_path = root / "repo/docs/helper-allowance.json"
        observation_path = root / "repo/docs/host-observation.json"
        plan = json.loads(plan_path.read_bytes())
        allowance = json.loads(allowance_path.read_bytes())
        observation = json.loads(observation_path.read_bytes())
        record["register_sha256"] = digest(path)
        record["digests"] = {
            "plan": {"stored": stored["context"]["plan_digest"], "current": plan["plan_digest"]},
            "allowance": {"stored": stored["context"]["allowance_digest"], "current": digest(allowance_path)},
            "observation": {"stored": stored["context"]["host_observation_digest"], "current": digest(observation_path)},
        }
        record["digest_matches"] = {key: value["stored"] == value["current"] for key, value in record["digests"].items()}
        record["stored_assignment_model_efforts"] = [[row["model"], row["reasoning_effort"]] for row in stored["allowance"]["assignments"]]
        record["current_assignment_model_efforts"] = [[row["model"], row["reasoning_effort"]] for row in allowance["assignments"]]
        record["stored_observed_model_efforts"] = [[row["model"], row["reasoning_effort"]] for row in stored["host_observation"]["supported_assignments"]]
        record["current_observed_model_efforts"] = [[row["model"], row["reasoning_effort"]] for row in observation["supported_assignments"]]
        records.append(record)
    output = artifact / ("helper-fixture-reuse-diagnostic-02.json" if matched else "helper-fixture-reuse-diagnostic-01.json")
    if output.exists():
        raise ValueError("IMMUTABLE_DIAGNOSTIC_EXISTS")
    output.write_bytes((json.dumps({"scope": "two exact generated retained roots; metadata/digest comparison only", "records": records}, indent=2) + "\n").encode("ascii"))
    print(json.dumps({"output": str(output), "records": records}, sort_keys=True))


if __name__ == "__main__":
    main()
