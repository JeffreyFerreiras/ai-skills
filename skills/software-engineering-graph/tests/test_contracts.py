import copy
import os
import json
import re
import unittest
from pathlib import Path

from graph_engine.config import (
    ENGINE_ARTIFACT_MAX, ENGINE_COLLECTION_MAX_BYTES, ENGINE_COLLECTION_MAX_MEMBERS,
    load_policy, role_capability_allowed,
)
from graph_engine.contracts import ContractError, validate_impact_map, validate_ref, validate_task_brief
from graph_engine.ids import sha256_bytes

from tests.test_support import GraphCase


class HelperProfileContractsTests(unittest.TestCase):
    """Static inventory/wiring checks, not enforcement of live-agent instructions."""

    def test_supported_profiles_and_parent_contract_wiring(self):
        from tests.test_standalone_acceptance import PROFILE_AGENTS
        from graph_engine.execution import NODE_ROLES
        from graph_engine.config import ENGINE_ROLE_CAPABILITIES

        root = Path(__file__).resolve().parents[1]
        helpers = {"evidence_scout", "validation_executor"}
        original = {"impact_mapper", "tech_lead", "software_architect", "senior_engineer",
                    "code_reviewer", "test_engineer", "security_reviewer"}
        self.assertEqual(PROFILE_AGENTS, {name + ".toml" for name in original | helpers})
        self.assertEqual(PROFILE_AGENTS, {p.name for p in (root / "profile-agents").iterdir()})
        expected = {
            "tech_lead": {"evidence_scout"}, "software_architect": {"evidence_scout"},
            "senior_engineer": helpers, "code_reviewer": {"evidence_scout"},
            "test_engineer": helpers, "security_reviewer": {"evidence_scout"},
        }
        matrix = {}
        contract = (root / "references/economy-helpers.md").read_text(encoding="utf-8")
        for parent, scout, executor in re.findall(r"^\| (\w+) \| (yes|no) \| (yes|no) \|$", contract, re.M):
            matrix[parent] = {name for name, enabled in (("evidence_scout", scout),
                                                        ("validation_executor", executor)) if enabled == "yes"}
        self.assertEqual(matrix, expected)
        for name in original | helpers:
            text = (root / "profile-agents" / (name + ".toml")).read_text(encoding="utf-8")
            match = re.search(r"^Approved helper types: ([\w, ]+)\.$", text, re.M)
            allowed = set(match.group(1).split(", ")) if match else set()
            self.assertEqual(allowed, expected.get(name, set()), name)
            if allowed:
                self.assertIn("references/economy-helpers.md", text)
        self.assertTrue(helpers.isdisjoint(NODE_ROLES))
        self.assertTrue(helpers.isdisjoint(NODE_ROLES.values()))
        self.assertTrue(helpers.isdisjoint(ENGINE_ROLE_CAPABILITIES))

    def test_helper_contract_sources_and_host_defaults(self):
        from graph_engine.hosts import resolve_assignment

        root = Path(__file__).resolve().parents[1]
        for name, sandbox in (("evidence_scout", "read-only"),
                              ("validation_executor", "workspace-write")):
            text = (root / "profile-agents" / (name + ".toml")).read_text(encoding="utf-8")
            model = re.search(r'^model = "([^"]+)"$', text, re.M).group(1)
            effort = re.search(r'^model_reasoning_effort = "([^"]+)"$', text, re.M).group(1)
            self.assertEqual((model, effort), resolve_assignment("codex-astra", "economy", "max"))
            self.assertIn('sandbox_mode = "' + sandbox + '"', text)
            self.assertEqual(text.count('"""'), 2)
            self.assertIn("references/economy-helpers.md", text)
        # Safety semantics are exercised by H01-H15 on an actual constrained host.
        scenarios = (root / "references/behavioral-evaluations.md").read_text(encoding="utf-8")
        self.assertEqual(set(re.findall(r"^### H(\d+):", scenarios, re.M)),
                         {"{:02d}".format(index) for index in range(1, 16)})


def _validate_json_schema(value, schema, root, path="$", seen_refs=None):
    """Validate the repository fixture's schema subset with the standard library only."""
    seen_refs = set() if seen_refs is None else seen_refs
    if "$ref" in schema:
        ref = schema["$ref"]
        if not ref.startswith("#/"):
            raise AssertionError("unsupported schema reference at " + path)
        target = root
        for part in ref[2:].split("/"):
            target = target[part]
        marker = (ref, path)
        if marker in seen_refs:
            raise AssertionError("recursive schema reference at " + path)
        return _validate_json_schema(value, target, root, path, seen_refs | {marker})

    if "anyOf" in schema or "oneOf" in schema:
        keyword = "anyOf" if "anyOf" in schema else "oneOf"
        errors = []
        matches = 0
        for candidate in schema[keyword]:
            try:
                _validate_json_schema(value, candidate, root, path, seen_refs)
                matches += 1
            except AssertionError as error:
                errors.append(str(error))
        if matches == 0:
            raise AssertionError("{}: no {} alternative matched ({})".format(path, keyword, "; ".join(errors)))
        if keyword == "oneOf" and matches != 1:
            raise AssertionError("{}: multiple oneOf alternatives matched".format(path))

    if "not" in schema:
        try:
            _validate_json_schema(value, schema["not"], root, path, seen_refs)
        except AssertionError:
            pass
        else:
            raise AssertionError("{}: forbidden schema matched".format(path))

    if "const" in schema and value != schema["const"]:
        raise AssertionError("{}: expected {!r}, got {!r}".format(path, schema["const"], value))
    if "enum" in schema and value not in schema["enum"]:
        raise AssertionError("{}: value is outside enum".format(path))

    expected_type = schema.get("type")
    if expected_type is not None:
        type_matches = {
            "object": lambda item: isinstance(item, dict),
            "array": lambda item: isinstance(item, list),
            "string": lambda item: isinstance(item, str),
            "integer": lambda item: isinstance(item, int) and not isinstance(item, bool),
            "boolean": lambda item: isinstance(item, bool),
            "null": lambda item: item is None,
        }
        expected_types = expected_type if isinstance(expected_type, list) else [expected_type]
        if not any(item in type_matches and type_matches[item](value) for item in expected_types):
            raise AssertionError("{}: expected {}".format(path, expected_type))

    if isinstance(value, dict):
        required = schema.get("required", [])
        missing = [key for key in required if key not in value]
        if missing:
            raise AssertionError("{}: missing {}".format(path, ",".join(missing)))
        properties = schema.get("properties", {})
        if schema.get("additionalProperties") is False:
            unknown = set(value) - set(properties)
            if unknown:
                raise AssertionError("{}: unknown {}".format(path, ",".join(sorted(unknown))))
        elif isinstance(schema.get("additionalProperties"), dict):
            additional_schema = schema["additionalProperties"]
            for key in sorted(set(value) - set(properties)):
                _validate_json_schema(
                    value[key], additional_schema, root, path + "." + key, seen_refs
                )
        for key, child_schema in properties.items():
            if key in value:
                _validate_json_schema(value[key], child_schema, root, path + "." + key, seen_refs)
        if len(value) < schema.get("minProperties", 0):
            raise AssertionError("{}: too few properties".format(path))
    elif isinstance(value, list):
        if len(value) < schema.get("minItems", 0) or len(value) > schema.get("maxItems", len(value)):
            raise AssertionError("{}: invalid item count".format(path))
        if schema.get("uniqueItems"):
            encoded = [json.dumps(item, sort_keys=True, separators=(",", ":")) for item in value]
            if len(encoded) != len(set(encoded)):
                raise AssertionError("{}: duplicate items".format(path))
        if "items" in schema:
            for index, item in enumerate(value):
                _validate_json_schema(item, schema["items"], root, "{}[{}]".format(path, index), seen_refs)
    elif isinstance(value, str):
        if len(value) < schema.get("minLength", 0) or len(value) > schema.get("maxLength", len(value)):
            raise AssertionError("{}: invalid string length".format(path))
        if "pattern" in schema and re.search(schema["pattern"], value) is None:
            raise AssertionError("{}: pattern mismatch".format(path))
    elif isinstance(value, int) and not isinstance(value, bool):
        if value < schema.get("minimum", value) or value > schema.get("maximum", value):
            raise AssertionError("{}: number outside bounds".format(path))


class ContractTests(GraphCase):
    def setUp(self):
        super().setUp()
        self.policy, self.snapshot = load_policy(self.repo)

    def policy_v2(self):
        policy = copy.deepcopy(self.policy)
        policy["schema_version"] = 2
        policy["artifact_roots"]["repo"] = ["artifacts/"]
        policy["implementation_roots"] = ["specs/", "app/", "tools/", "pyproject.toml"]
        policy["required_checks"] = {
            "focused": {
                "command_id": "python-focused", "mandatory": True,
                "argv": ["python", "-m", "unittest", "tests.test_contracts"],
                "timeout_seconds": 600,
            }
        }
        replacements = {
            "repo:docs/": "repo:specs/", "repo:src/": "repo:app/",
            "repo:scripts/": "repo:tools/", "repo:docs/technical-designs/": "repo:artifacts/design/",
            "npm-run-check": "python-focused",
        }
        for capabilities in policy["role_capabilities"].values():
            for capability in capabilities:
                capability["target_ref"] = replacements.get(
                    capability["target_ref"], capability["target_ref"],
                )
        for relative in ("artifacts/design", "specs", "app", "tools"):
            (self.repo / relative).mkdir(parents=True, exist_ok=True)
        (self.repo / "pyproject.toml").write_text("[tool.test]\n", encoding="utf-8")
        policy["role_capabilities"]["senior_engineer"].append(
            {"effect": "filesystem_read", "action": "read", "target_ref": "repo:pyproject.toml"}
        )
        policy["role_capabilities"]["senior_engineer"].append(
            {"effect": "filesystem_write", "action": "edit", "target_ref": "repo:pyproject.toml"}
        )
        return policy

    def test_real_policy_and_task_validate(self):
        task = validate_task_brief(self.task(tags=["security_privacy"]), self.snapshot.digest, self.policy)
        self.assertEqual(task["mandatory_impact_tags"], ["security_privacy"])

    def test_v2_task_requires_exact_structured_model_sizing(self):
        task = validate_task_brief(self.task_v2(), self.snapshot.digest, self.policy)
        self.assertEqual(
            task["model_sizing"], {"scope_extent": "bounded", "uncertainty": "low"},
        )
        for mutation, code in (
            (lambda value: value.pop("model_sizing"), "MISSING_FIELD"),
            (lambda value: value["model_sizing"].update(extra="no"), "UNKNOWN_FIELD"),
            (lambda value: value["model_sizing"].update(scope_extent="global"), "UNKNOWN_VALUE"),
        ):
            with self.subTest(code=code):
                candidate = self.task_v2()
                mutation(candidate)
                with self.assertRaisesRegex(ContractError, code):
                    validate_task_brief(candidate, self.snapshot.digest, self.policy)

        legacy = self.task()
        legacy["model_sizing"] = {"scope_extent": "bounded", "uncertainty": "low"}
        with self.assertRaisesRegex(ContractError, "UNKNOWN_FIELD"):
            validate_task_brief(legacy, self.snapshot.digest, self.policy)

    def test_v3_task_requires_and_binds_one_helper_allowance(self):
        allowance_path = self.repo / "docs" / "helper-allowance.json"
        allowance_path.write_text("{}", encoding="utf-8")
        task = self.task_v2()
        task["schema_version"] = 3
        task["helper_allowance"] = {
            "ref": "repo:docs/helper-allowance.json",
            "sha256": sha256_bytes(allowance_path.read_bytes()),
        }
        validated = validate_task_brief(task, self.snapshot.digest, self.policy)
        self.assertEqual(validated["helper_allowance"], task["helper_allowance"])
        from graph_engine.execution import build_execution_plan
        plan = build_execution_plan("RUN-1", validated)
        self.assertEqual(plan["schema_version"], 3)
        self.assertEqual(plan["helper_allowance"], task["helper_allowance"])
        for mutation, code in (
            (lambda value: value.pop("helper_allowance"), "MISSING_FIELD"),
            (lambda value: value["helper_allowance"].update(extra="no"), "UNKNOWN_FIELD"),
            (lambda value: value["helper_allowance"].update(ref="authority:test"), "REPOSITORY_REF_REQUIRED"),
        ):
            candidate = copy.deepcopy(task)
            mutation(candidate)
            with self.subTest(code=code), self.assertRaisesRegex(ContractError, code):
                validate_task_brief(candidate, self.snapshot.digest, self.policy)

    def test_published_task_schema_accepts_exact_v1_and_v2_contracts(self):
        schema = json.loads(
            (Path(__file__).parents[1] / "references" / "task-brief.schema.json").read_text(
                encoding="utf-8"
            )
        )
        _validate_json_schema(self.task(), schema, schema)
        _validate_json_schema(self.task_v2(), schema, schema)
        task_v3 = self.task_v2()
        task_v3["schema_version"] = 3
        task_v3["helper_allowance"] = {
            "ref": "repo:docs/helper-allowance.json", "sha256": "a" * 64,
        }
        _validate_json_schema(task_v3, schema, schema)
        legacy_with_v2_field = self.task()
        legacy_with_v2_field["model_sizing"] = {
            "scope_extent": "bounded", "uncertainty": "low",
        }
        with self.assertRaises(AssertionError):
            _validate_json_schema(legacy_with_v2_field, schema, schema)

    def test_mapper_cannot_downgrade_or_remove_tag(self):
        task = validate_task_brief(self.task(tags=["security_privacy"]), self.snapshot.digest, self.policy)
        with self.assertRaisesRegex(ContractError, "ROUTE_DOWNGRADE"):
            validate_impact_map({"schema_version": 1, "task_id": "TASK-1", "route_label": "fast_path", "impact_tags": ["security_privacy"], "evidence_refs": [], "attempt_id": "attempt", "claim_digest": "a" * 64}, task, self.policy)
        with self.assertRaisesRegex(ContractError, "MANDATORY_TAG_REMOVED"):
            validate_impact_map({"schema_version": 1, "task_id": "TASK-1", "route_label": "full_delivery", "impact_tags": [], "evidence_refs": [], "attempt_id": "attempt", "claim_digest": "a" * 64}, task, self.policy)
        fast_task = validate_task_brief(self.task(route="fast_path"), self.snapshot.digest, self.policy)
        with self.assertRaisesRegex(ContractError, "FAST_PATH_INVARIANT"):
            validate_impact_map({"schema_version": 1, "task_id": "TASK-1", "route_label": "fast_path", "impact_tags": ["security_privacy"], "evidence_refs": [], "attempt_id": "attempt", "claim_digest": "a" * 64}, fast_task, self.policy)

    def test_advisory_task_cannot_receive_write_authority(self):
        task = self.task("advisory", "advisory")
        task["authority"]["capabilities"] = [
            {"effect": "filesystem_write", "action": "edit", "target_ref": "repo:docs/"}
        ]
        with self.assertRaisesRegex(ContractError, "ADVISORY_MUST_BE_READ_ONLY"):
            validate_task_brief(task, self.snapshot.digest, self.policy)

    def test_secret_fields_and_urls_are_rejected_without_echo(self):
        task = self.task()
        task["password"] = "do-not-echo"
        with self.assertRaises(ContractError) as captured:
            validate_task_brief(task, self.snapshot.digest, self.policy)
        self.assertNotIn("do-not-echo", str(captured.exception))
        with self.assertRaisesRegex(ContractError, "URL_FORBIDDEN"):
            validate_ref("https://example.invalid/token", "ref")

    def test_policy_cannot_raise_loop_limit(self):
        policy = copy.deepcopy(self.policy)
        policy["limits"]["design_revisions"] = 4
        (self.repo / ".codex" / "engineering-graph.json").write_text(__import__("json").dumps(policy), encoding="utf-8")
        with self.assertRaisesRegex(ContractError, "LIMIT_MAY_NOT_INCREASE"):
            load_policy(self.repo)

    def test_policy_cannot_add_engine_authority(self):
        policy = copy.deepcopy(self.policy)
        policy["role_capabilities"]["senior_engineer"].append(
            {"effect": "external_write", "action": "deploy", "target_ref": "production"}
        )
        (self.repo / ".codex" / "engineering-graph.json").write_text(__import__("json").dumps(policy), encoding="utf-8")
        with self.assertRaisesRegex(ContractError, "ENGINE_AUTHORITY_EXCEEDED"):
            load_policy(self.repo)

    def test_repository_added_checks_cannot_expand_command_capabilities(self):
        policy = copy.deepcopy(self.policy)
        policy["required_checks"]["repo-added"] = {
            "command_id": "repository-added-command", "mandatory": True,
        }
        policy["role_capabilities"]["senior_engineer"].append({
            "effect": "command", "action": "run", "target_ref": "repository-added-command",
        })
        (self.repo / ".codex" / "engineering-graph.json").write_text(
            json.dumps(policy), encoding="utf-8"
        )
        with self.assertRaisesRegex(ContractError, "ENGINE_COMMAND_SET_CHANGED|ENGINE_AUTHORITY_EXCEEDED"):
            load_policy(self.repo)
        from graph_engine.config import ENGINE_ROLE_CAPABILITIES
        self.assertNotIn(("command", "run", "*"), ENGINE_ROLE_CAPABILITIES["senior_engineer"])

    def test_policy_v2_allows_bounded_repo_paths_and_exact_configured_commands(self):
        policy = self.policy_v2()
        policy_path = self.repo / ".codex" / "engineering-graph.json"
        policy_path.write_text(json.dumps(policy), encoding="utf-8")
        loaded, snapshot = load_policy(self.repo)
        self.assertEqual(loaded["schema_version"], 2)
        self.assertEqual(loaded["required_checks"]["focused"]["command_id"], "python-focused")
        task = self.task()
        task["policy_approval"]["sha256"] = snapshot.digest
        task["authority"]["capabilities"] = [
            {"effect": "filesystem_read", "action": "read", "target_ref": "repo:pyproject.toml"},
            {"effect": "filesystem_write", "action": "edit", "target_ref": "repo:pyproject.toml"},
            {"effect": "filesystem_write", "action": "edit", "target_ref": "repo:app/"},
            {"effect": "command", "action": "run", "target_ref": "python-focused"},
        ]
        task["required_check_ids"] = ["focused"]
        validated = validate_task_brief(task, snapshot.digest, loaded)
        from graph_engine.planner import NodeSpec, envelope
        branch = envelope(
            "RUN-1", snapshot.digest, loaded, validated,
            NodeSpec("senior_engineer", "senior_engineer", "implementation", 0),
            "ready", [],
        )
        self.assertEqual(branch["effect_capabilities"], validated["authority"]["capabilities"])

    def test_policy_v2_rejects_artifact_implementation_overlap_and_unclassified_writes(self):
        policy_path = self.repo / ".codex" / "engineering-graph.json"
        overlap_pairs = (
            (["artifacts/"], ["artifacts/"]),
            (["artifacts/"], ["artifacts/design/"]),
            (["artifacts/design/"], ["artifacts/"]),
            (["shared"], ["shared/"]),
        )
        for implementation_roots, artifact_roots in overlap_pairs:
            policy = self.policy_v2()
            policy["implementation_roots"] = implementation_roots
            policy["artifact_roots"]["repo"] = artifact_roots
            policy_path.write_text(json.dumps(policy), encoding="utf-8")
            with self.subTest(
                implementation=implementation_roots, artifacts=artifact_roots,
            ), self.assertRaisesRegex(ContractError, "ARTIFACT_IMPLEMENTATION_OVERLAP"):
                load_policy(self.repo)

        policy = self.policy_v2()
        (self.repo / "unclassified").mkdir()
        policy["role_capabilities"]["senior_engineer"].append(
            {"effect": "filesystem_write", "action": "edit", "target_ref": "repo:unclassified/"}
        )
        policy_path.write_text(json.dumps(policy), encoding="utf-8")
        with self.assertRaisesRegex(ContractError, "ENGINE_AUTHORITY_EXCEEDED"):
            load_policy(self.repo)

    def test_policy_v2_uses_platform_path_identity_for_classification_and_duplicates(self):
        policy_path = self.repo / ".codex" / "engineering-graph.json"
        case_insensitive = os.path.normcase("Artifacts") == os.path.normcase("artifacts")
        case_overlaps = (
            (["Artifacts/"], ["artifacts/"]),
            (["Artifacts/Design/"], ["artifacts/"]),
            (["Artifacts/"], ["artifacts/design/"]),
        )
        for implementation_roots, artifact_roots in case_overlaps:
            policy = self.policy_v2()
            policy["implementation_roots"] = implementation_roots
            policy["artifact_roots"]["repo"] = artifact_roots
            policy_path.write_text(json.dumps(policy), encoding="utf-8")
            if case_insensitive:
                with self.subTest(
                    implementation=implementation_roots, artifacts=artifact_roots,
                ), self.assertRaisesRegex(ContractError, "ARTIFACT_IMPLEMENTATION_OVERLAP"):
                    load_policy(self.repo)

        duplicate_sets = (["generated", "generated/"],)
        if case_insensitive:
            duplicate_sets += (["RootFile", "rootfile"],)
        for implementation_roots in duplicate_sets:
            policy = self.policy_v2()
            policy["implementation_roots"] = list(implementation_roots)
            policy_path.write_text(json.dumps(policy), encoding="utf-8")
            with self.subTest(implementation=implementation_roots), self.assertRaisesRegex(
                ContractError, "INVALID_OR_DUPLICATE",
            ):
                load_policy(self.repo)

        if case_insensitive:
            policy = self.policy_v2()
            policy["artifact_roots"]["repo"] = ["Artifacts/", "artifacts/"]
            policy_path.write_text(json.dumps(policy), encoding="utf-8")
            with self.assertRaisesRegex(ContractError, "INVALID_OR_DUPLICATE"):
                load_policy(self.repo)

        policy = self.policy_v2()
        policy["implementation_roots"] = ["App/"]
        capability = {
            "effect": "filesystem_write", "action": "edit", "target_ref": "repo:app/module.py",
        }
        self.assertEqual(
            role_capability_allowed(policy, "senior_engineer", capability), case_insensitive,
        )
        policy["implementation_roots"] = ["package.json"]
        self.assertTrue(role_capability_allowed(
            policy, "senior_engineer", {**capability, "target_ref": "repo:package.json/"},
        ))
        self.assertFalse(role_capability_allowed(
            policy, "senior_engineer", {**capability, "target_ref": "repo:package.json/child"},
        ))

    def test_policy_v2_accepts_empty_implementation_roots_without_implementation_writes(self):
        policy = self.policy_v2()
        policy["implementation_roots"] = []
        policy["role_capabilities"]["senior_engineer"] = [
            capability
            for capability in policy["role_capabilities"]["senior_engineer"]
            if capability["effect"] != "filesystem_write"
            or capability["target_ref"].startswith("repo:artifacts/")
        ]
        policy_path = self.repo / ".codex" / "engineering-graph.json"
        policy_path.write_text(json.dumps(policy), encoding="utf-8")
        loaded, _snapshot = load_policy(self.repo)
        self.assertEqual(loaded["implementation_roots"], [])

    def test_policy_v1_rejects_v2_implementation_classification(self):
        policy = copy.deepcopy(self.policy)
        policy["implementation_roots"] = []
        (self.repo / ".codex" / "engineering-graph.json").write_text(
            json.dumps(policy), encoding="utf-8",
        )
        with self.assertRaisesRegex(ContractError, "UNKNOWN_FIELD"):
            load_policy(self.repo)

    def test_policy_v2_rejects_path_command_and_role_escalations(self):
        mutations = (
            lambda p: p["role_capabilities"]["senior_engineer"].append(
                {"effect": "filesystem_write", "action": "edit", "target_ref": "repo:.codex/"}
            ),
            lambda p: p["role_capabilities"]["tech_lead"].append(
                {"effect": "filesystem_write", "action": "edit", "target_ref": "repo:app/"}
            ),
            lambda p: p["role_capabilities"]["code_reviewer"].append(
                {"effect": "filesystem_write", "action": "edit", "target_ref": "repo:app/"}
            ),
            lambda p: p["role_capabilities"]["senior_engineer"].append(
                {"effect": "command", "action": "run", "target_ref": "not-configured"}
            ),
            lambda p: p["role_capabilities"]["senior_engineer"].append(
                {"effect": "external_write", "action": "deploy", "target_ref": "production"}
            ),
        )
        policy_path = self.repo / ".codex" / "engineering-graph.json"
        for index, mutation in enumerate(mutations):
            with self.subTest(index=index):
                policy = self.policy_v2()
                mutation(policy)
                policy_path.write_text(json.dumps(policy), encoding="utf-8")
                with self.assertRaises(ContractError):
                    load_policy(self.repo)

    def test_policy_v2_rejects_link_escape_and_requires_complete_check_specs(self):
        outside = self.root / "outside"
        outside.mkdir()
        link = self.repo / "linked"
        try:
            link.symlink_to(outside, target_is_directory=True)
        except (OSError, NotImplementedError):
            link = None
        policy_path = self.repo / ".codex" / "engineering-graph.json"
        if link is not None:
            policy = self.policy_v2()
            policy["role_capabilities"]["senior_engineer"].append(
                {"effect": "filesystem_write", "action": "edit", "target_ref": "repo:linked/"}
            )
            policy_path.write_text(json.dumps(policy), encoding="utf-8")
            with self.assertRaisesRegex(ContractError, "LINK_OR_REPARSE_POINT"):
                load_policy(self.repo)
        for missing in ("argv", "timeout_seconds"):
            policy = self.policy_v2()
            del policy["required_checks"]["focused"][missing]
            policy_path.write_text(json.dumps(policy), encoding="utf-8")
            with self.assertRaisesRegex(ContractError, "MISSING_FIELD"):
                load_policy(self.repo)
        artifact_file = self.repo / "artifact-file"
        artifact_file.write_text("not a directory", encoding="utf-8")
        policy = self.policy_v2()
        policy["artifact_roots"]["repo"] = ["artifact-file/"]
        policy_path.write_text(json.dumps(policy), encoding="utf-8")
        with self.assertRaisesRegex(ContractError, "DIRECTORY_ROOT_REQUIRED"):
            load_policy(self.repo)

    def test_policy_v1_bytes_digest_and_exact_target_ceiling_remain_frozen(self):
        self.assertEqual(self.snapshot.digest, sha256_bytes(self.policy_bytes))
        policy = copy.deepcopy(self.policy)
        policy["role_capabilities"]["senior_engineer"].append(
            {"effect": "filesystem_write", "action": "edit", "target_ref": "repo:app/"}
        )
        (self.repo / "app").mkdir()
        (self.repo / ".codex" / "engineering-graph.json").write_text(json.dumps(policy), encoding="utf-8")
        with self.assertRaisesRegex(ContractError, "ENGINE_AUTHORITY_EXCEEDED"):
            load_policy(self.repo)

    def test_oversized_manifest_is_rejected(self):
        self.initialize()
        branch = self.claim()
        oversized = self.store.inbox_root("albanian-live-translate", "RUN-1") / "oversized.json"
        oversized.write_text('{"padding":"' + ("x" * 270000) + '"}', encoding="utf-8")
        if os.name != "nt":
            os.chmod(oversized, 0o600)
        with self.assertRaisesRegex(ContractError, "FILE_TOO_LARGE"):
            self.graphctl("record", "branch-result", "--run-id", "RUN-1", "--branch-id", branch["branch_id"], "--attempt-id", branch["attempt_id"], "--claim-token", branch["claim_token"], "--result-manifest", str(oversized), "--op-id", "oversized-1")

    def test_repository_policy_reaches_required_roles_and_checks(self):
        roles = {item["role"] for item in self.policy["node_templates"].values()}
        self.assertTrue({"impact_mapper", "tech_lead", "software_architect", "senior_engineer", "code_reviewer", "test_engineer", "supervisor"}.issubset(roles))
        for node_key in ("design_research_architecture", "design_research_validation"):
            template = self.policy["node_templates"][node_key]
            self.assertEqual(
                (template["role"], template["stages"], template["max_retries"]),
                ("impact_mapper", ["research"], 1),
            )
            self.assertEqual(
                {key: template["output_contract"][key] for key in (
                    "artifact_required", "evidence_required", "decision_forbidden", "findings_forbidden",
                )},
                {"artifact_required": True, "evidence_required": True,
                 "decision_forbidden": True, "findings_forbidden": True},
            )
        self.assertEqual(self.policy["required_checks"]["repo-check"]["command_id"], "npm-run-check")
        self.assertEqual(set(self.policy["specialists"]), {"audio_realtime_translation", "ios_webkit_native", "release_operations", "security_privacy"})
        schema = json.loads((Path(__file__).parents[1] / "references" / "repository-config.schema.json").read_text(encoding="utf-8"))
        self.assertEqual(set(schema["required"]), set(self.policy))
        self.assertFalse(schema["additionalProperties"])
        maximum_reviewers = 2 + len(self.policy["specialists"])
        self.assertEqual(ENGINE_COLLECTION_MAX_MEMBERS, maximum_reviewers)
        self.assertEqual(
            self.policy["artifact_kinds"]["collection"]["max_bytes"],
            ENGINE_COLLECTION_MAX_BYTES,
        )
        self.assertGreaterEqual(
            ENGINE_COLLECTION_MAX_BYTES,
            maximum_reviewers * ENGINE_ARTIFACT_MAX["branch_result"] + 64 * 1024,
        )

    def test_repository_fixture_matches_published_schema(self):
        schema_path = Path(__file__).parents[1] / "references" / "repository-config.schema.json"
        fixture_path = Path(__file__).parent / "fixtures" / "engineering-graph.json"
        schema = json.loads(schema_path.read_text(encoding="utf-8"))
        fixture = json.loads(fixture_path.read_text(encoding="utf-8"))
        _validate_json_schema(fixture, schema, schema)
        fixture_v2 = self.policy_v2()
        _validate_json_schema(fixture_v2, schema, schema)
        fixture["implementation_roots"] = []
        with self.assertRaises(AssertionError):
            _validate_json_schema(fixture, schema, schema)

    def test_repository_fixture_rejects_schema_missing_research_flags(self):
        schema_path = Path(__file__).parents[1] / "references" / "repository-config.schema.json"
        fixture_path = Path(__file__).parent / "fixtures" / "engineering-graph.json"
        schema = json.loads(schema_path.read_text(encoding="utf-8"))
        fixture = json.loads(fixture_path.read_text(encoding="utf-8"))
        for field in ("evidence_required", "decision_forbidden", "findings_forbidden"):
            with self.subTest(field=field):
                mutated = copy.deepcopy(schema)
                del mutated["$defs"]["outputContract"]["properties"][field]
                with self.assertRaisesRegex(AssertionError, "unknown"):
                    _validate_json_schema(fixture, mutated, mutated)

    def test_skill_dispatch_requirements_are_explicit(self):
        path = Path(__file__).parents[1] / "SKILL.md"
        content = path.read_text(encoding="utf-8")
        start = "<!-- dispatch-transparency:start -->"
        end = "<!-- dispatch-transparency:end -->"
        self.assertEqual(content.count(start), 1, path)
        self.assertEqual(content.count(end), 1, path)
        block = content.split(start, 1)[1].split(end, 1)[0].strip()
        required_phrases = (
            "Immediately before every dispatch",
            "concrete agent or task name",
            "exact approved model",
            "exact approved reasoning effort",
            "bounded scope",
            "initial dispatch",
            "fan-out member",
            "retry",
            "replacement",
            "follow-up",
            "same-role continuation",
            "Refuse the dispatch",
            "unavailable, unverifiable, or mismatched",
        )
        for phrase in required_phrases:
            self.assertIn(phrase, block, phrase)

    def test_policy_missing_research_template_fails_closed(self):
        policy = copy.deepcopy(self.policy)
        del policy["node_templates"]["design_research_validation"]
        (self.repo / ".codex" / "engineering-graph.json").write_text(
            json.dumps(policy), encoding="utf-8"
        )
        with self.assertRaisesRegex(ContractError, "ENGINE_TOPOLOGY_CHANGED"):
            load_policy(self.repo)

    def test_policy_topology_roots_contracts_and_targets_are_engine_bounded(self):
        mutations = [
            lambda p: p["routes"]["full_delivery"].update(entry_node="senior_engineer"),
            lambda p: p["artifact_roots"].update(repo=["src/"]),
            lambda p: p["node_templates"]["architect"]["output_contract"].update(artifact_kind="technical_design"),
            lambda p: p["specialists"]["security_privacy"].update(mandatory=False),
            lambda p: p["artifact_kinds"]["finding"].update(extensions=[".exe"]),
            lambda p: p["compatible_engine"].update(min="3.0.0"),
            lambda p: p["role_capabilities"]["senior_engineer"].append(
                {"effect": "filesystem_write", "action": "edit", "target_ref": "repo:.codex/"}
            ),
            lambda p: p["role_capabilities"]["senior_engineer"].append(
                {"effect": "command", "action": "run", "target_ref": "unknown-command"}
            ),
        ]
        for index, mutate in enumerate(mutations):
            with self.subTest(index=index):
                policy = copy.deepcopy(self.policy)
                mutate(policy)
                (self.repo / ".codex" / "engineering-graph.json").write_text(json.dumps(policy), encoding="utf-8")
                with self.assertRaises(ContractError):
                    load_policy(self.repo)
                (self.repo / ".codex" / "engineering-graph.json").write_bytes(self.policy_bytes)

    def test_task_is_minimized_and_external_artifacts_are_reverified(self):
        task = self.task()
        task["scope"] = {
            "included": ["customer-private-project-name"],
            "excluded": ["confidential-future-acquisition"],
        }
        self.initialize_task(task)
        db = self.store.db_path("albanian-live-translate", "RUN-1")
        with self.store.connect(db) as connection:
            stored = json.loads(connection.execute("SELECT task_json FROM runs").fetchone()[0])
        self.assertNotIn("objective", stored)
        self.assertNotIn("user_outcome", stored)
        self.assertNotIn("constraints", stored)
        self.assertNotIn("scope_ids", stored)
        database_bytes = db.read_bytes()
        self.assertNotIn(b"customer-private-project-name", database_bytes)
        self.assertNotIn(b"confidential-future-acquisition", database_bytes)
        self.assertEqual(stored["acceptance_ids"], ["AC-001"])
        (self.repo / "docs" / "engineering-graph.md").write_text("changed", encoding="utf-8")
        with self.assertRaisesRegex(Exception, "INPUT_DIGEST_MISMATCH"):
            self.graphctl("resume", "--run-id", "RUN-1", "--ack-degraded-permissions", "--ack-degraded-durability")

    def test_nonexistent_and_digest_mismatched_artifacts_are_rejected(self):
        self.initialize(); self.impact("full_delivery")
        missing = "ledger:missing#sha256=" + ("a" * 64)
        with self.assertRaisesRegex(ContractError, "LEDGER_ARTIFACT_NOT_FOUND"):
            self.graphctl("record", "acceptance-evidence", "--run-id", "RUN-1", "--criterion-id", "AC-001", "--artifact-ref", missing, "--artifact-sha256", "a" * 64, "--op-id", "missing-evidence")
        artifact = self.repo_artifact("acceptance_evidence", "digest-mismatch")
        with self.assertRaisesRegex(ContractError, "DIGEST_DISAGREEMENT"):
            self.graphctl("record", "acceptance-evidence", "--run-id", "RUN-1", "--criterion-id", "AC-001", "--artifact-ref", artifact["ref"], "--artifact-sha256", "b" * 64, "--op-id", "mismatch-evidence")
