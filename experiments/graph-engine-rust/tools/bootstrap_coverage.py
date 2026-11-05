"""Create an explicit pending coverage-map patch from the frozen inventory."""

import argparse
import hashlib
import json
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--inventory", type=Path, required=True)
    args = parser.parse_args()
    raw = args.inventory.read_bytes()
    inventory = json.loads(raw)
    modules, definitions = {}, {}
    for module in inventory["modules"]:
        name = Path(module["path"]).stem
        component = ("compat" if name == "ids" else "domain") + "/" + name
        base = {"oracle_path": module["path"], "oracle_sha256": module["sha256"],
                "native_component": None, "planned_component": component,
                "native_assertions": [], "fixtures": [], "negative_proof": [],
                "proof_artifacts": [], "status": "not_started"}
        modules[name] = dict(base, oracle_lines=[1, module["lines"]])
        for definition in module["definitions"]:
            definitions[name + "." + definition["name"]] = dict(base,
                oracle_lines=[definition["line"], definition["end_line"]])
    commands = {}
    for field, family in (("graph_cli_leaves", "graphctl"),
                          ("helper_register_cli_leaves", "helper-register")):
        for command in inventory[field]:
            commands[family + " " + command["command"]] = {
                "oracle_options": command["options"], "native_component": None,
                "native_assertions": [], "fixtures": [], "negative_proof": [],
                "proof_artifacts": [], "status": "not_started"}
    tests = {}
    for identities in inventory["test_groups"].values():
        for identity in identities:
            tests[identity] = {"boundary": None, "native_component": None,
                "native_assertions": [], "fixtures": [], "negative_proof": [],
                "nondeterministic_relationships": [], "proof_artifacts": [],
                "status": "not_started"}
    if (len(modules), len(commands), len(tests)) != (17, 39, 360):
        raise ValueError("FROZEN_INVENTORY_MISMATCH")
    result = {"schema_version": 1, "oracle_revision": inventory["head"],
              "inventory_sha256": hashlib.sha256(raw).hexdigest(),
              "counts": {"modules": 17, "definitions": len(definitions),
                         "graph_cli": 34, "helper_cli": 5, "mutation_rules": 24,
                         "behavior_identities": 359, "hygiene_identities": 1},
              "modules": modules, "definitions": definitions, "commands": commands,
              "mutation_rules": {}, "test_identities": tests,
              "gaps": ["All behavior starts pending. Inventory is not proof.",
                       "Mutation names/state rules await authorized state source inspection."]}
    content = json.dumps(result, separators=(",", ":"), sort_keys=True) + "\n"
    print("*** Begin Patch")
    target = Path(__file__).resolve().parent.parent / "parity/coverage-map.json"
    print("*** Add File: " + target.as_posix())
    for line in content.splitlines():
        print("+" + line)
    print("*** End Patch")


if __name__ == "__main__":
    main()
