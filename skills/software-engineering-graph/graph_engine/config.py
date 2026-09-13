"""Exhaustive repository policy loading and engine-owned validation."""

import fnmatch
import os
from pathlib import Path
from pathlib import PurePosixPath
from typing import Any, Dict, Mapping, Set, Tuple

from . import ENGINE_VERSION
from .contracts import (
    ContractError, EFFECTS, RISK_TAGS, Snapshot, ensure_safe_components, lexical_relative,
    opaque, require_keys, safe_json_snapshot, validate_ref,
)
from .reviewer_delegation import validate_policy_config


ENGINE_ROUTES = {
    "advisory": {"entry_node": "advisory_reviewer", "design_gates": False, "delivery_gates": False, "closure": "closure"},
    "design_only": {"entry_node": "tech_lead", "design_gates": True, "delivery_gates": False, "closure": "closure"},
    "fast_path": {"entry_node": "senior_engineer", "design_gates": False, "delivery_gates": True, "closure": "closure"},
    "full_delivery": {"entry_node": "tech_lead", "design_gates": True, "delivery_gates": True, "closure": "closure"},
}

ENGINE_NODE_SPECS = {
    "impact_mapper": ("impact_mapper", {"bootstrap"}, "impact_map", False, (), 1),
    "design_research_architecture": ("impact_mapper", {"research"}, "evidence_manifest", True, (), 1),
    "design_research_validation": ("impact_mapper", {"research"}, "evidence_manifest", True, (), 1),
    "advisory_reviewer": ("code_reviewer", {"advisory"}, "advisory_report", True, (), 1),
    "tech_lead": ("tech_lead", {"design"}, "technical_design", True, (), 1),
    "architect": ("software_architect", {"design"}, "design_review", True, ("APPROVE", "REVISE", "BLOCK"), 1),
    "senior_engineer": ("senior_engineer", {"implementation"}, "implementation_handoff", False, ("IMPLEMENTED", "REDESIGN_REQUIRED"), 1),
    "code_reviewer": ("code_reviewer", {"delivery"}, "delivery_review", True, ("APPROVE", "REVISE", "BLOCK"), 1),
    "test_engineer": ("test_engineer", {"delivery"}, "delivery_review", True, ("APPROVE", "REVISE", "BLOCK"), 1),
    "audio_realtime_specialist": ("audio_realtime_specialist", {"design", "delivery"}, "specialist_review", True, ("APPROVE", "REVISE", "BLOCK"), 1),
    "ios_platform_specialist": ("ios_platform_specialist", {"design", "delivery"}, "specialist_review", True, ("APPROVE", "REVISE", "BLOCK"), 1),
    "release_operations_reviewer": ("release_operations_reviewer", {"design", "delivery"}, "specialist_review", True, ("APPROVE", "REVISE", "BLOCK"), 1),
    "security_reviewer": ("security_reviewer", {"design", "delivery"}, "specialist_review", True, ("APPROVE", "REVISE", "BLOCK"), 1),
    "supervisor_design_consolidation": ("supervisor", {"design"}, "design_consolidation", False, (), 0),
    "supervisor_delivery_consolidation": ("supervisor", {"delivery"}, "delivery_consolidation", False, (), 0),
}

ENGINE_RESEARCH_NODES = {
    "design_research_architecture": "architecture",
    "design_research_validation": "validation",
}

ENGINE_ROLE_CAPABILITIES = {
    "impact_mapper": {
        ("filesystem_read", "read", "repo:docs/"),
        ("external_read", "inspect", "andromeda"),
    },
    "tech_lead": {
        ("filesystem_read", "read", "repo:docs/"),
        ("filesystem_read", "read", "repo:src/"),
        ("filesystem_write", "edit", "repo:docs/technical-designs/"),
    },
    "software_architect": {
        ("filesystem_read", "read", "repo:docs/"),
        ("filesystem_read", "read", "repo:src/"),
    },
    "senior_engineer": {
        ("filesystem_read", "read", "repo:docs/"),
        ("filesystem_read", "read", "repo:src/"),
        ("filesystem_write", "edit", "repo:docs/"),
        ("filesystem_write", "edit", "repo:scripts/"),
        ("filesystem_write", "edit", "repo:src/"),
        ("command", "run", "npm-run-check"),
    },
    "code_reviewer": {
        ("filesystem_read", "read", "repo:docs/"),
        ("filesystem_read", "read", "repo:src/"),
        ("command", "run", "npm-run-check"),
    },
    "test_engineer": {
        ("filesystem_read", "read", "repo:docs/"),
        ("filesystem_read", "read", "repo:src/"),
        ("command", "run", "npm-run-check"),
    },
    "audio_realtime_specialist": {
        ("filesystem_read", "read", "repo:docs/"),
        ("filesystem_read", "read", "repo:src/"),
    },
    "ios_platform_specialist": {
        ("filesystem_read", "read", "repo:docs/"),
        ("filesystem_read", "read", "repo:src/"),
    },
    "release_operations_reviewer": {
        ("filesystem_read", "read", "repo:docs/"),
        ("external_read", "inspect", "andromeda"),
    },
    "security_reviewer": {
        ("filesystem_read", "read", "repo:docs/"),
        ("filesystem_read", "read", "repo:src/"),
    },
    "supervisor": {("filesystem_read", "read", "repo:docs/")},
}

ENGINE_REQUIRED_CHECKS = {
    "repo-check": {"command_id": "npm-run-check", "mandatory": True},
}

ENGINE_DEFAULT_BRANCH_LEASE_SECONDS = 900
ENGINE_MAX_BRANCH_LEASE_SECONDS = 3600

ENGINE_SPECIALISTS = {
    "audio_realtime_translation": ("audio_realtime_specialist", "audio_realtime_specialist"),
    "ios_webkit_native": ("ios_platform_specialist", "ios_platform_specialist"),
    "release_operations": ("release_operations_reviewer", "release_operations_reviewer"),
    "security_privacy": ("security_reviewer", "security_reviewer"),
}

ENGINE_DENIALS = {
    "**/.env", "**/.env.*", "**/.git/**", "**/.hg/**", "**/.ssh/**", "**/.svn/**",
    "**/*credential*", "**/*secret*", "**/*.key", "**/*.p12", "**/*.pem", "**/*.pfx",
    "**/graph-inbox/**", "**/graph-runs/**", "**/id_ed25519", "**/id_rsa",
}

ENGINE_COLLECTION_MAX_MEMBERS = 6
ENGINE_COLLECTION_MAX_BYTES = 2 * 1024 * 1024

ENGINE_ARTIFACT_MAX = {
    "task_brief": 128 * 1024, "evidence_manifest": 128 * 1024,
    "technical_design": 4 * 1024 * 1024,
    "impact_map": 256 * 1024, "advisory_report": 256 * 1024,
    "design_review": 256 * 1024, "implementation_handoff": 256 * 1024,
    "delivery_review": 256 * 1024, "specialist_review": 256 * 1024,
    "design_consolidation": 256 * 1024, "delivery_consolidation": 256 * 1024,
    "collection": ENGINE_COLLECTION_MAX_BYTES, "acceptance_evidence": 256 * 1024,
    "check_evidence": 256 * 1024,
    "finding": 256 * 1024, "failure": 256 * 1024,
    "branch_result": 256 * 1024,
    "timeout": 128 * 1024, "skip": 128 * 1024, "block": 128 * 1024,
}

ENGINE_ARTIFACT_EXTENSIONS = {
    "task_brief": {".json"}, "evidence_manifest": {".json"},
    "technical_design": {".json", ".md"}, "impact_map": {".json"},
    "advisory_report": {".json", ".md"}, "design_review": {".json", ".md"},
    "implementation_handoff": {".json", ".md"}, "delivery_review": {".json", ".md"},
    "specialist_review": {".json", ".md"}, "design_consolidation": {".json"},
    "delivery_consolidation": {".json"}, "collection": {".json"},
    "acceptance_evidence": {".json", ".md", ".txt"},
    "check_evidence": {".json", ".md", ".txt"},
    "finding": {".json", ".md", ".txt"}, "failure": {".json", ".md", ".txt"},
    "branch_result": {".json"},
    "timeout": {".json", ".md", ".txt"}, "skip": {".json", ".md", ".txt"},
    "block": {".json", ".md", ".txt"},
}


def version_tuple(value: str) -> Tuple[int, int, int]:
    try:
        parts = tuple(int(part) for part in value.split("."))
    except (AttributeError, ValueError):
        raise ContractError("compatible_engine", "INVALID_VERSION")
    if len(parts) != 3:
        raise ContractError("compatible_engine", "INVALID_VERSION")
    return parts


def engine_version_compatible(value: str, policy: Mapping[str, Any]) -> bool:
    compatible = policy["compatible_engine"]
    candidate = version_tuple(value)
    return version_tuple(compatible["min"]) <= candidate < version_tuple(compatible["max_exclusive"])


def _validate_output_contract(node_key: str, value: Mapping[str, Any]) -> None:
    role, stages, kind, artifact_required, decisions, retry_ceiling = ENGINE_NODE_SPECS[node_key]
    allowed = {"artifact_kind", "schema_version", "artifact_required"}
    if decisions:
        allowed.add("decision_values")
    if node_key == "senior_engineer":
        allowed.update({"artifact_required_for_decisions", "evidence_required_for_decisions"})
    if node_key in ENGINE_RESEARCH_NODES:
        allowed.update({"evidence_required", "decision_forbidden", "findings_forbidden"})
    require_keys(value, allowed, allowed, f"node_templates.{node_key}.output_contract")
    expected = {"artifact_kind": kind, "schema_version": 1, "artifact_required": artifact_required}
    if decisions:
        expected["decision_values"] = list(decisions)
    if node_key == "senior_engineer":
        expected["artifact_required_for_decisions"] = ["IMPLEMENTED"]
        expected["evidence_required_for_decisions"] = ["REDESIGN_REQUIRED"]
    if node_key in ENGINE_RESEARCH_NODES:
        expected.update({
            "evidence_required": True,
            "decision_forbidden": True,
            "findings_forbidden": True,
        })
    if dict(value) != expected:
        raise ContractError(f"node_templates.{node_key}.output_contract", "ENGINE_CONTRACT_CHANGED")


def _validate_capability_shape(capability: Any, role: str, command_ids: Set[str]) -> Tuple[str, str, str]:
    if not isinstance(capability, dict):
        raise ContractError(f"role_capabilities.{role}", "INVALID_OBJECT")
    require_keys(capability, {"effect", "action", "target_ref"}, {"effect", "action", "target_ref"}, f"role_capabilities.{role}")
    effect, action, target = capability["effect"], capability["action"], capability["target_ref"]
    if effect not in EFFECTS:
        raise ContractError(f"role_capabilities.{role}", "ENGINE_AUTHORITY_EXCEEDED")
    if effect.startswith("filesystem"):
        validate_ref(target, "capability.target_ref")
        if not target.startswith(("repo:", "profile:software-engineering-graph/")):
            raise ContractError("capability.target_ref", "FILESYSTEM_REF_REQUIRED")
    elif effect == "command":
        if opaque(target, "capability.target_ref") not in command_ids:
            raise ContractError("capability.target_ref", "UNKNOWN_COMMAND_TARGET")
    else:
        opaque(target, "capability.target_ref")
    return effect, action, target


def _repository_target(repo: Path, target: str, denied_patterns: Any) -> str:
    if not target.startswith("repo:"):
        raise ContractError("capability.target_ref", "REPOSITORY_REF_REQUIRED")
    relative = lexical_relative(target[5:], "capability.target_ref")
    if any(character in relative for character in "*?[]{}"):
        raise ContractError("capability.target_ref", "WILDCARD_FORBIDDEN")
    normalized = relative.lower()
    path = PurePosixPath(normalized)
    for pattern in denied_patterns:
        lowered = pattern.lower()
        if fnmatch.fnmatchcase(normalized, lowered) or path.match(lowered):
            raise ContractError("capability.target_ref", "SENSITIVE_PATH")
    if normalized.startswith((".codex/", ".agents/", ".claude/", ".cursor/")):
        raise ContractError("capability.target_ref", "PROFILE_OR_POLICY_ROOT_FORBIDDEN")
    repository = repo.resolve(strict=True)
    candidate = repo / relative.rstrip("/")
    ensure_safe_components(candidate, repository)
    try:
        candidate.resolve(strict=False).relative_to(repository)
    except ValueError:
        raise ContractError("capability.target_ref", "OUTSIDE_REPOSITORY")
    return "repo:" + relative


def _location_key(path: str) -> str:
    """Return a platform-aware key for one repository-relative location."""
    return os.path.normcase(path.rstrip("/"))


def _under_configured_root(target: str, roots: Any) -> bool:
    if not target.startswith("repo:"):
        return False
    relative = target[5:]
    target_key = _location_key(relative)
    for root in roots:
        root_key = _location_key(root)
        if target_key == root_key:
            return True
        if root.endswith("/") and target_key.startswith(root_key + os.sep):
            return True
    return False


def _roots_overlap(left: str, right: str) -> bool:
    left_key = _location_key(left)
    right_key = _location_key(right)
    return (
        left_key == right_key
        or (left.endswith("/") and right_key.startswith(left_key + os.sep))
        or (right.endswith("/") and left_key.startswith(right_key + os.sep))
    )


def _has_location_duplicates(paths: Any) -> bool:
    keys = [_location_key(path) for path in paths]
    return len(keys) != len(set(keys))


def role_capability_allowed(
    policy: Mapping[str, Any], role: str, capability: Mapping[str, Any],
) -> bool:
    """Apply the same versioned role ceiling during validation and envelope planning."""
    candidate = (capability["effect"], capability["action"], capability["target_ref"])
    ceiling = ENGINE_ROLE_CAPABILITIES.get(role, set())
    if policy["schema_version"] == 1:
        return candidate in ceiling
    effect, action, target = candidate
    allowed_effect_actions = {(item[0], item[1]) for item in ceiling}
    if (effect, action) not in allowed_effect_actions:
        return False
    if effect == "command":
        return target in {check["command_id"] for check in policy["required_checks"].values()}
    if effect.startswith("filesystem"):
        if not target.startswith("repo:"):
            return False
        if effect == "filesystem_write":
            if role == "senior_engineer":
                return _under_configured_root(
                    target, policy["implementation_roots"] + policy["artifact_roots"]["repo"],
                )
            return role == "tech_lead" and _under_configured_root(
                target, policy["artifact_roots"]["repo"],
            )
        return True
    return candidate in ceiling


def load_policy(repo: Path) -> Tuple[Dict[str, Any], Snapshot]:
    root = repo.absolute()
    snapshot = safe_json_snapshot(root / ".codex" / "engineering-graph.json", [root], 256 * 1024)
    value = snapshot.parsed
    if not isinstance(value, dict):
        raise ContractError("policy", "INVALID_OBJECT")
    required = {
        "schema_version", "repository_id", "compatible_engine", "artifact_roots",
        "artifact_kinds", "impact_tags", "routes", "node_templates", "specialists",
        "limits", "required_checks", "role_capabilities", "denied_patterns",
    }
    policy_version = value.get("schema_version")
    if type(policy_version) is not int or policy_version not in {1, 2}:
        require_keys(value, required, required | {"reviewer_delegation", "implementation_roots"}, "policy")
        raise ContractError("policy.schema_version", "UNSUPPORTED_SCHEMA")
    version_required = required | ({"implementation_roots"} if policy_version == 2 else set())
    version_allowed = version_required | {"reviewer_delegation"}
    require_keys(value, version_required, version_allowed, "policy")
    opaque(value["repository_id"], "repository_id")
    compatible = value["compatible_engine"]
    if not isinstance(compatible, dict):
        raise ContractError("compatible_engine", "INVALID_OBJECT")
    require_keys(compatible, {"min", "max_exclusive"}, {"min", "max_exclusive"}, "compatible_engine")
    if not engine_version_compatible(ENGINE_VERSION, value):
        raise ContractError("compatible_engine", "ENGINE_INCOMPATIBLE")
    roots = value["artifact_roots"]
    if not isinstance(roots, dict):
        raise ContractError("artifact_roots", "INVALID_OBJECT")
    require_keys(roots, {"repo", "profile"}, {"repo", "profile"}, "artifact_roots")
    for root_kind in ("repo", "profile"):
        configured = roots[root_kind]
        if not isinstance(configured, list) or not configured or len(configured) != len(set(configured)):
            raise ContractError(f"artifact_roots.{root_kind}", "INVALID_OR_DUPLICATE")
        normalized_configured = []
        for path in configured:
            normalized = lexical_relative(path, f"artifact_roots.{root_kind}")
            normalized_configured.append(normalized)
            if not normalized.endswith("/"):
                raise ContractError(f"artifact_roots.{root_kind}", "DIRECTORY_ROOT_REQUIRED")
            if root_kind == "profile" and not normalized.startswith("references/"):
                raise ContractError("artifact_roots.profile", "PROFILE_ROOT_FORBIDDEN")
            engine_root = "docs/" if root_kind == "repo" else "references/"
            if policy_version == 1 and not normalized.startswith(engine_root):
                raise ContractError(f"artifact_roots.{root_kind}", "ENGINE_ROOT_EXCEEDED")
            if root_kind == "repo" and policy_version == 2:
                configured_denials = value["denied_patterns"]
                _repository_target(
                    root, "repo:" + normalized,
                    configured_denials if isinstance(configured_denials, list) else ENGINE_DENIALS,
                )
                artifact_root = root / normalized.rstrip("/")
                if artifact_root.exists() and not artifact_root.is_dir():
                    raise ContractError("artifact_roots.repo", "DIRECTORY_ROOT_REQUIRED")
        if policy_version == 2:
            if _has_location_duplicates(normalized_configured):
                raise ContractError(f"artifact_roots.{root_kind}", "INVALID_OR_DUPLICATE")
            roots[root_kind] = normalized_configured
    if policy_version == 2:
        configured = value["implementation_roots"]
        if not isinstance(configured, list) or len(configured) != len(set(configured)):
            raise ContractError("implementation_roots", "INVALID_OR_DUPLICATE")
        normalized_roots = []
        for path in configured:
            normalized = lexical_relative(path, "implementation_roots")
            if not normalized.endswith("/") and "/" in normalized:
                raise ContractError("implementation_roots", "DIRECTORY_OR_ROOT_FILE_REQUIRED")
            _repository_target(
                root, "repo:" + normalized,
                value["denied_patterns"] if isinstance(value["denied_patterns"], list) else ENGINE_DENIALS,
            )
            candidate = root / normalized.rstrip("/")
            if normalized.endswith("/") and candidate.exists() and not candidate.is_dir():
                raise ContractError("implementation_roots", "DIRECTORY_ROOT_REQUIRED")
            if not normalized.endswith("/") and candidate.exists() and not candidate.is_file():
                raise ContractError("implementation_roots", "ROOT_FILE_REQUIRED")
            normalized_roots.append(normalized)
        if _has_location_duplicates(normalized_roots):
            raise ContractError("implementation_roots", "INVALID_OR_DUPLICATE")
        for implementation_root in normalized_roots:
            if any(_roots_overlap(implementation_root, artifact_root) for artifact_root in roots["repo"]):
                raise ContractError("implementation_roots", "ARTIFACT_IMPLEMENTATION_OVERLAP")
        value["implementation_roots"] = normalized_roots
    if not isinstance(value["impact_tags"], list) or set(value["impact_tags"]) != RISK_TAGS or len(value["impact_tags"]) != len(RISK_TAGS):
        raise ContractError("impact_tags", "ENGINE_INVARIANT_CHANGED")
    if value["routes"] != ENGINE_ROUTES:
        raise ContractError("routes", "ENGINE_TOPOLOGY_CHANGED")
    templates = value["node_templates"]
    if not isinstance(templates, dict) or set(templates) != set(ENGINE_NODE_SPECS):
        raise ContractError("node_templates", "ENGINE_TOPOLOGY_CHANGED")
    for key, expected in ENGINE_NODE_SPECS.items():
        template = templates[key]
        if not isinstance(template, dict):
            raise ContractError(f"node_templates.{key}", "INVALID_OBJECT")
        require_keys(template, {"role", "stages", "mandatory", "max_retries", "output_contract"}, {"role", "stages", "mandatory", "max_retries", "output_contract"}, f"node_templates.{key}")
        role, stages, kind, artifact_required, decisions, retry_ceiling = expected
        if template["role"] != role or set(template["stages"]) != stages or template["mandatory"] is not True:
            raise ContractError(f"node_templates.{key}", "ENGINE_NODE_BINDING_CHANGED")
        if isinstance(template["max_retries"], bool) or not isinstance(template["max_retries"], int) or not 0 <= template["max_retries"] <= retry_ceiling:
            raise ContractError(f"node_templates.{key}.max_retries", "RETRY_CEILING_EXCEEDED")
        _validate_output_contract(key, template["output_contract"])
    specialists = value["specialists"]
    if not isinstance(specialists, dict) or set(specialists) != set(ENGINE_SPECIALISTS):
        raise ContractError("specialists", "ENGINE_INVARIANT_CHANGED")
    for tag, (node_key, role) in ENGINE_SPECIALISTS.items():
        expected = {"node_key": node_key, "role": role, "stages": ["design", "delivery"], "mandatory": True}
        if specialists[tag] != expected:
            raise ContractError("specialists." + tag, "ENGINE_INVARIANT_CHANGED")
    limits = value["limits"]
    if not isinstance(limits, dict):
        raise ContractError("limits", "INVALID_OBJECT")
    require_keys(
        limits,
        {"design_revisions", "delivery_repairs", "inspection", "manifest_bytes", "artifact_bytes"},
        {"design_revisions", "delivery_repairs", "inspection", "manifest_bytes", "artifact_bytes", "branch_lease_seconds"},
        "limits",
    )
    if "branch_lease_seconds" in limits:
        if isinstance(limits["branch_lease_seconds"], bool) or not isinstance(limits["branch_lease_seconds"], int) or not 30 <= limits["branch_lease_seconds"] <= ENGINE_MAX_BRANCH_LEASE_SECONDS:
            raise ContractError("limits.branch_lease_seconds", "INVALID_LEASE")
    if isinstance(limits["design_revisions"], bool) or not isinstance(limits["design_revisions"], int) or not 0 <= limits["design_revisions"] <= 3:
        raise ContractError("limits.design_revisions", "LIMIT_MAY_NOT_INCREASE")
    if isinstance(limits["delivery_repairs"], bool) or not isinstance(limits["delivery_repairs"], int) or not 0 <= limits["delivery_repairs"] <= 3:
        raise ContractError("limits.delivery_repairs", "LIMIT_MAY_NOT_INCREASE")
    inspection = limits["inspection"]
    if not isinstance(inspection, dict):
        raise ContractError("limits.inspection", "INVALID_OBJECT")
    require_keys(inspection, {"file_reads", "discovery_commands"}, {"file_reads", "discovery_commands"}, "limits.inspection")
    for key, ceiling in (("file_reads", 12), ("discovery_commands", 8)):
        if isinstance(inspection[key], bool) or not isinstance(inspection[key], int) or not 0 <= inspection[key] <= ceiling:
            raise ContractError("limits.inspection." + key, "LIMIT_MAY_NOT_INCREASE")
    if not isinstance(limits["manifest_bytes"], int) or not 1 <= limits["manifest_bytes"] <= 256 * 1024:
        raise ContractError("limits.manifest_bytes", "LIMIT_MAY_NOT_INCREASE")
    if not isinstance(limits["artifact_bytes"], int) or not 1 <= limits["artifact_bytes"] <= 4 * 1024 * 1024:
        raise ContractError("limits.artifact_bytes", "LIMIT_MAY_NOT_INCREASE")
    kinds = value["artifact_kinds"]
    if not isinstance(kinds, dict) or set(kinds) != set(ENGINE_ARTIFACT_MAX):
        raise ContractError("artifact_kinds", "ENGINE_ARTIFACT_POLICY_CHANGED")
    for kind, ceiling in ENGINE_ARTIFACT_MAX.items():
        config = kinds[kind]
        if not isinstance(config, dict):
            raise ContractError("artifact_kinds." + kind, "INVALID_OBJECT")
        require_keys(config, {"extensions", "max_bytes"}, {"extensions", "max_bytes"}, "artifact_kinds." + kind)
        extensions = config["extensions"]
        if not isinstance(extensions, list) or not extensions or len(extensions) != len(set(extensions)) or any(not isinstance(ext, str) or not ext.startswith(".") or ext != ext.lower() for ext in extensions):
            raise ContractError("artifact_kinds." + kind, "INVALID_EXTENSIONS")
        if not set(extensions).issubset(ENGINE_ARTIFACT_EXTENSIONS[kind]):
            raise ContractError("artifact_kinds." + kind, "ENGINE_ARTIFACT_TYPE_EXCEEDED")
        if not isinstance(config["max_bytes"], int) or not 1 <= config["max_bytes"] <= ceiling:
            raise ContractError("artifact_kinds." + kind, "LIMIT_MAY_NOT_INCREASE")
    checks = value["required_checks"]
    if not isinstance(checks, dict) or not checks:
        raise ContractError("required_checks", "ENGINE_COMMAND_SET_CHANGED")
    if policy_version == 1 and not set(ENGINE_REQUIRED_CHECKS).issubset(checks):
        raise ContractError("required_checks", "ENGINE_COMMAND_SET_CHANGED")
    command_ids: Set[str] = set()
    for check_id, check in checks.items():
        opaque(check_id, "required_checks")
        if not isinstance(check, dict):
            raise ContractError("required_checks." + check_id, "INVALID_OBJECT")
        require_keys(
            check,
            {"command_id", "mandatory"},
            {"command_id", "mandatory", "argv", "timeout_seconds"},
            "required_checks." + check_id,
        )
        command_id = opaque(check["command_id"], "required_checks.command_id")
        if (policy_version == 1 and check_id in ENGINE_REQUIRED_CHECKS
                and command_id != ENGINE_REQUIRED_CHECKS[check_id]["command_id"]):
            raise ContractError("required_checks." + check_id, "ENGINE_COMMAND_SET_CHANGED")
        if command_id in command_ids or check["mandatory"] is not True:
            raise ContractError("required_checks", "INVALID_OR_DUPLICATE")
        if policy_version == 2 and "argv" not in check:
            raise ContractError("required_checks." + check_id, "MISSING_FIELD")
        if "argv" in check:
            argv = check["argv"]
            if not isinstance(argv, list) or not argv or len(argv) > 32 or any(not isinstance(item, str) or not item or len(item) > 1024 for item in argv):
                raise ContractError("required_checks." + check_id + ".argv", "INVALID_COMMAND")
        if policy_version == 2 and "timeout_seconds" not in check:
            raise ContractError("required_checks." + check_id, "MISSING_FIELD")
        if "timeout_seconds" in check:
            timeout = check["timeout_seconds"]
            if isinstance(timeout, bool) or not isinstance(timeout, int) or not 1 <= timeout <= 3600:
                raise ContractError("required_checks." + check_id + ".timeout_seconds", "INVALID_TIMEOUT")
        command_ids.add(command_id)
    capabilities = value["role_capabilities"]
    if not isinstance(capabilities, dict) or set(capabilities) != set(ENGINE_ROLE_CAPABILITIES):
        raise ContractError("role_capabilities", "ENGINE_AUTHORITY_CHANGED")
    for role, configured in capabilities.items():
        if not isinstance(configured, list):
            raise ContractError("role_capabilities." + role, "INVALID_LIST")
        tuples = []
        for item in configured:
            capability_tuple = _validate_capability_shape(item, role, command_ids)
            if policy_version == 2 and item["effect"].startswith("filesystem"):
                configured_denials = value["denied_patterns"]
                normalized_target = _repository_target(
                    root, item["target_ref"],
                    configured_denials if isinstance(configured_denials, list) else ENGINE_DENIALS,
                )
                item["target_ref"] = normalized_target
                capability_tuple = (item["effect"], item["action"], normalized_target)
            if not role_capability_allowed(value, role, item):
                raise ContractError(f"role_capabilities.{role}", "ENGINE_AUTHORITY_EXCEEDED")
            tuples.append(capability_tuple)
        if len(tuples) != len(set(tuples)):
            raise ContractError("role_capabilities." + role, "DUPLICATE_VALUE")
    denials = value["denied_patterns"]
    if not isinstance(denials, list) or len(denials) != len(set(denials)) or not ENGINE_DENIALS.issubset(set(denials)):
        raise ContractError("denied_patterns", "ENGINE_DENIAL_REMOVED")
    for pattern in denials:
        if not isinstance(pattern, str) or not pattern or len(pattern) > 256:
            raise ContractError("denied_patterns", "INVALID_PATTERN")
    reviewer_delegation = validate_policy_config(value.get("reviewer_delegation"))
    if reviewer_delegation is not None:
        value["reviewer_delegation"] = reviewer_delegation
    return value, snapshot


def branch_lease_seconds(policy: Mapping[str, Any]) -> int:
    return int(policy.get("limits", {}).get("branch_lease_seconds", ENGINE_DEFAULT_BRANCH_LEASE_SECONDS))
