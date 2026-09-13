"""Deterministic host-only helper reservation register.

The register records trusted-caller evidence. It does not prove host confinement,
execute helpers, or grant authority beyond an approved allowance.
"""

import json
import os
import stat
import tempfile
from contextlib import contextmanager
from pathlib import Path
from typing import Any, Callable, Dict, Iterator, List, Mapping, Optional, Sequence, Tuple

from .contracts import (
    ContractError, bounded_string, digest, ensure_safe_components, lexical_relative, opaque,
    require_keys, safe_file_snapshot, safe_json_snapshot, validate_ref,
)
from .ids import canonical_bytes, repository_digest, sha256_bytes
from .hosts import dispatch_model, known_hosts
from .state import StateError, local_filesystem_identity, repository_identity


MAX_RECORD_BYTES = 256 * 1024
ELIGIBLE_PARENTS = {
    "tech_lead", "software_architect", "senior_engineer", "code_reviewer",
    "test_engineer", "security_reviewer",
}
HELPER_ROLES = {"evidence_scout", "validation_executor"}
HOST_CAPABILITIES = {
    "fresh_model_effort_selection", "filesystem_confinement", "tool_confinement",
    "command_confinement",
}
LIMIT_KEYS = {
    "children", "concurrency", "commands", "time_seconds", "output_tokens", "file_reads",
}
TERMINAL_STATES = {"succeeded", "failed", "blocked", "cancelled", "replaced"}
UNTRUSTED_CAPABILITY_SOURCES = ("caller", "prompt", "self-report", "self_report", "agent-report", "agent_report")


class HelperRegisterError(RuntimeError):
    def __init__(self, code: str):
        self.code = code
        super().__init__(code)


def _integer(value: Any, field: str, *, minimum: int = 0, maximum: int = 2 ** 31 - 1) -> int:
    if type(value) is not int or not minimum <= value <= maximum:
        raise ContractError(field, "INVALID_LIMIT")
    return value


def _unique_ids(value: Any, field: str, *, allow_empty: bool = True) -> List[str]:
    if not isinstance(value, list) or (not allow_empty and not value):
        raise ContractError(field, "INVALID_LIST")
    result = [opaque(item, field) for item in value]
    if len(result) != len(set(result)):
        raise ContractError(field, "DUPLICATE_VALUE")
    return result


def _scope_refs(value: Any, field: str) -> List[str]:
    if not isinstance(value, list) or not value:
        raise ContractError(field, "INVALID_LIST")
    result = [validate_ref(item, field) for item in value]
    if len(result) != len(set(result)):
        raise ContractError(field, "DUPLICATE_VALUE")
    if any(not item.startswith(("repo:", "profile:software-engineering-graph/")) for item in result):
        raise ContractError(field, "FILESYSTEM_REF_REQUIRED")
    return result


def _limits(value: Any, field: str) -> Dict[str, int]:
    if not isinstance(value, dict):
        raise ContractError(field, "INVALID_OBJECT")
    require_keys(value, LIMIT_KEYS, LIMIT_KEYS, field)
    limits = {key: _integer(value[key], field + "." + key) for key in LIMIT_KEYS}
    if limits["children"] < 1 or limits["concurrency"] < 1:
        raise ContractError(field, "INVALID_LIMIT")
    return limits


def _commands(value: Any, field: str) -> List[Dict[str, Any]]:
    if not isinstance(value, list) or len(value) > 32:
        raise ContractError(field, "INVALID_LIST")
    result = []
    identifiers = set()
    for item in value:
        if not isinstance(item, dict):
            raise ContractError(field, "INVALID_OBJECT")
        require_keys(
            item, {"command_id", "argv", "timeout_seconds"},
            {"command_id", "argv", "timeout_seconds"}, field,
        )
        command_id = opaque(item["command_id"], field + ".command_id")
        argv = item["argv"]
        if (not isinstance(argv, list) or not argv or len(argv) > 32
                or any(not isinstance(arg, str) or not arg or len(arg) > 1024 for arg in argv)):
            raise ContractError(field + ".argv", "INVALID_COMMAND")
        timeout = _integer(item["timeout_seconds"], field + ".timeout_seconds", minimum=1, maximum=3600)
        if command_id in identifiers:
            raise ContractError(field, "DUPLICATE_VALUE")
        identifiers.add(command_id)
        result.append({"command_id": command_id, "argv": list(argv), "timeout_seconds": timeout})
    return result


def _parent_capabilities(value: Any, field: str) -> List[Dict[str, str]]:
    if not isinstance(value, list) or not value:
        raise ContractError(field, "INVALID_LIST")
    result = []
    seen = set()
    for item in value:
        if not isinstance(item, dict):
            raise ContractError(field, "INVALID_OBJECT")
        require_keys(item, {"effect", "action", "target_ref"}, {"effect", "action", "target_ref"}, field)
        effect = item["effect"]
        action = item["action"]
        if (effect, action) not in {("filesystem_read", "read"), ("command", "run")}:
            raise ContractError(field, "PARENT_CAPABILITY_FORBIDDEN")
        target = (
            validate_ref(item["target_ref"], field + ".target_ref")
            if effect == "filesystem_read"
            else opaque(item["target_ref"], field + ".target_ref")
        )
        key = (effect, action, target)
        if key in seen:
            raise ContractError(field, "DUPLICATE_VALUE")
        seen.add(key)
        result.append({"effect": effect, "action": action, "target_ref": target})
    return sorted(result, key=lambda item: (item["effect"], item["action"], item["target_ref"]))


def _ref_within(scope_ref: str, ceiling_ref: str) -> bool:
    if scope_ref == ceiling_ref:
        return True
    if not ceiling_ref.endswith("/"):
        return False
    return scope_ref.startswith(ceiling_ref)


def _trusted_observation_source(source: str) -> bool:
    lowered = source.lower()
    return not any(marker in lowered for marker in UNTRUSTED_CAPABILITY_SOURCES)


def validate_allowance(value: Any, run_id: str, host: Optional[str] = None) -> Dict[str, Any]:
    if not isinstance(value, dict):
        raise ContractError("allowance", "INVALID_OBJECT")
    required = {
        "schema_version", "allowance_id", "run_id", "assignments",
        "shared_limits", "resources",
    }
    require_keys(value, required, required, "allowance")
    if value["schema_version"] != 1:
        raise ContractError("allowance.schema_version", "UNSUPPORTED_SCHEMA")
    result = {
        "schema_version": 1,
        "allowance_id": opaque(value["allowance_id"], "allowance.allowance_id"),
        "run_id": opaque(value["run_id"], "allowance.run_id"),
        "shared_limits": _limits(value["shared_limits"], "allowance.shared_limits"),
    }
    if result["run_id"] != run_id:
        raise ContractError("allowance", "PLAN_BINDING_MISMATCH")

    resources = value["resources"]
    if not isinstance(resources, list) or len(resources) > 64:
        raise ContractError("allowance.resources", "INVALID_LIST")
    resource_rows = []
    resource_keys = set()
    for item in resources:
        if not isinstance(item, dict):
            raise ContractError("allowance.resources", "INVALID_OBJECT")
        require_keys(item, {"key", "capacity"}, {"key", "capacity"}, "allowance.resources")
        key = opaque(item["key"], "allowance.resources.key")
        capacity = _integer(item["capacity"], "allowance.resources.capacity", minimum=1, maximum=1024)
        if key in resource_keys:
            raise ContractError("allowance.resources", "DUPLICATE_VALUE")
        resource_keys.add(key)
        resource_rows.append({"key": key, "capacity": capacity})
    result["resources"] = sorted(resource_rows, key=lambda item: item["key"])

    assignments = value["assignments"]
    if not isinstance(assignments, list) or not assignments or len(assignments) > 32:
        raise ContractError("allowance.assignments", "INVALID_LIST")
    assignment_rows = []
    assignment_ids = set()
    for item in assignments:
        if not isinstance(item, dict):
            raise ContractError("allowance.assignments", "INVALID_OBJECT")
        fields = {
            "assignment_id", "parent_role", "helper_role", "contract_revision", "model",
            "reasoning_effort", "parent_capabilities", "scope_refs", "commands",
            "checkpoint_policy", "resource_keys", "required_host_capabilities", "limits",
        }
        require_keys(item, fields, fields, "allowance.assignments")
        assignment_id = opaque(item["assignment_id"], "allowance.assignment_id")
        parent_role = opaque(item["parent_role"], "allowance.parent_role")
        helper_role = opaque(item["helper_role"], "allowance.helper_role")
        if assignment_id in assignment_ids:
            raise ContractError("allowance.assignments", "DUPLICATE_VALUE")
        if parent_role not in ELIGIBLE_PARENTS or helper_role not in HELPER_ROLES:
            raise ContractError("allowance.assignments", "HELPER_ROLE_FORBIDDEN")
        if helper_role == "validation_executor" and parent_role not in {"senior_engineer", "test_engineer"}:
            raise ContractError("allowance.assignments", "HELPER_ROLE_FORBIDDEN")
        if item["contract_revision"] != 1:
            raise ContractError("allowance.contract_revision", "UNSUPPORTED_SCHEMA")
        model = bounded_string(item["model"], "allowance.model", 128)
        reasoning_effort = opaque(item["reasoning_effort"], "allowance.reasoning_effort")
        if host is not None:
            try:
                dispatch_model(host, model, reasoning_effort)
            except ValueError:
                raise ContractError("allowance.assignments", "HELPER_ASSIGNMENT_MISMATCH")
        commands = _commands(item["commands"], "allowance.commands")
        if helper_role == "evidence_scout" and commands:
            raise ContractError("allowance.commands", "COMMAND_FORBIDDEN")
        if helper_role == "validation_executor" and not commands:
            raise ContractError("allowance.commands", "COMMAND_REQUIRED")
        required_caps = _unique_ids(
            item["required_host_capabilities"], "allowance.required_host_capabilities",
            allow_empty=False,
        )
        mandatory_caps = {
            "fresh_model_effort_selection", "filesystem_confinement", "tool_confinement",
        }
        if helper_role == "validation_executor":
            mandatory_caps.add("command_confinement")
        if not set(required_caps).issubset(HOST_CAPABILITIES) or not mandatory_caps.issubset(required_caps):
            raise ContractError("allowance.required_host_capabilities", "HOST_CAPABILITY_REQUIRED")
        keys = _unique_ids(item["resource_keys"], "allowance.resource_keys")
        if not set(keys).issubset(resource_keys):
            raise ContractError("allowance.resource_keys", "UNKNOWN_RESOURCE")
        limits = _limits(item["limits"], "allowance.assignment_limits")
        if any(limits[key] > result["shared_limits"][key] for key in LIMIT_KEYS):
            raise ContractError("allowance.assignment_limits", "LIMIT_MAY_NOT_INCREASE")
        if item["checkpoint_policy"] != "observed_repository_state":
            raise ContractError("allowance.checkpoint_policy", "UNKNOWN_VALUE")
        parent_capabilities = _parent_capabilities(
            item["parent_capabilities"], "allowance.parent_capabilities",
        )
        scopes = _scope_refs(item["scope_refs"], "allowance.scope_refs")
        read_ceilings = [
            cap["target_ref"] for cap in parent_capabilities
            if cap["effect"] == "filesystem_read"
        ]
        if any(not any(_ref_within(scope, ceiling) for ceiling in read_ceilings) for scope in scopes):
            raise ContractError("allowance.scope_refs", "PARENT_AUTHORITY_EXCEEDED")
        command_ceilings = {
            cap["target_ref"] for cap in parent_capabilities if cap["effect"] == "command"
        }
        if any(command["command_id"] not in command_ceilings for command in commands):
            raise ContractError("allowance.commands", "PARENT_AUTHORITY_EXCEEDED")
        assignment_rows.append({
            "assignment_id": assignment_id, "parent_role": parent_role,
            "helper_role": helper_role, "contract_revision": 1,
            "model": model, "reasoning_effort": reasoning_effort,
            "parent_capabilities": parent_capabilities, "scope_refs": scopes,
            "commands": commands, "checkpoint_policy": "observed_repository_state",
            "resource_keys": sorted(keys), "required_host_capabilities": sorted(required_caps),
            "limits": limits,
        })
        assignment_ids.add(assignment_id)
    result["assignments"] = sorted(assignment_rows, key=lambda item: item["assignment_id"])
    return result


def validate_host_observation(value: Any) -> Dict[str, Any]:
    if not isinstance(value, dict):
        raise ContractError("host_observation", "INVALID_OBJECT")
    fields = {
        "schema_version", "host_id", "observed_at", "source", "uncertainty",
        "capabilities", "supported_assignments",
    }
    require_keys(value, fields, fields, "host_observation")
    if value["schema_version"] != 1:
        raise ContractError("host_observation.schema_version", "UNSUPPORTED_SCHEMA")
    capabilities = value["capabilities"]
    if not isinstance(capabilities, dict) or not capabilities:
        raise ContractError("host_observation.capabilities", "INVALID_OBJECT")
    if not set(capabilities).issubset(HOST_CAPABILITIES):
        raise ContractError("host_observation.capabilities", "UNKNOWN_FIELD")
    normalized = {}
    for key, item in capabilities.items():
        if not isinstance(item, dict):
            raise ContractError("host_observation.capabilities", "INVALID_OBJECT")
        require_keys(item, {"status", "source", "uncertainty"}, {"status", "source", "uncertainty"}, key)
        if item["status"] not in {"verified", "unverified", "unavailable"}:
            raise ContractError("host_observation.capabilities.status", "UNKNOWN_VALUE")
        normalized[key] = {
            "status": item["status"],
            "source": bounded_string(item["source"], key + ".source", 256),
            "uncertainty": bounded_string(item["uncertainty"], key + ".uncertainty", 1024),
        }
    assignments = value["supported_assignments"]
    if not isinstance(assignments, list) or not assignments or len(assignments) > 32:
        raise ContractError("host_observation.supported_assignments", "INVALID_LIST")
    normalized_assignments = []
    seen_assignments = set()
    for item in assignments:
        if not isinstance(item, dict):
            raise ContractError("host_observation.supported_assignments", "INVALID_OBJECT")
        require_keys(
            item, {"model", "reasoning_effort", "status", "source", "uncertainty"},
            {"model", "reasoning_effort", "status", "source", "uncertainty"},
            "host_observation.supported_assignments",
        )
        model = bounded_string(item["model"], "host_observation.model", 128)
        effort = opaque(item["reasoning_effort"], "host_observation.reasoning_effort")
        if item["status"] not in {"verified", "unverified", "unavailable"}:
            raise ContractError("host_observation.assignment.status", "UNKNOWN_VALUE")
        pair = (model, effort)
        if pair in seen_assignments:
            raise ContractError("host_observation.supported_assignments", "DUPLICATE_VALUE")
        seen_assignments.add(pair)
        normalized_assignments.append({
            "model": model, "reasoning_effort": effort, "status": item["status"],
            "source": bounded_string(item["source"], "host_observation.assignment.source", 256),
            "uncertainty": bounded_string(item["uncertainty"], "host_observation.assignment.uncertainty", 1024),
        })
    return {
        "schema_version": 1,
        "host_id": opaque(value["host_id"], "host_observation.host_id"),
        "observed_at": bounded_string(value["observed_at"], "host_observation.observed_at", 64),
        "source": bounded_string(value["source"], "host_observation.source", 256),
        "uncertainty": bounded_string(value["uncertainty"], "host_observation.uncertainty", 1024),
        "capabilities": normalized,
        "supported_assignments": sorted(
            normalized_assignments, key=lambda item: (item["model"], item["reasoning_effort"]),
        ),
    }


def _validate_plan(value: Any, run_id: str) -> Dict[str, Any]:
    if not isinstance(value, dict) or value.get("schema_version") != 3:
        raise ContractError("execution_plan", "HELPER_PLAN_V3_REQUIRED")
    if value.get("run_id") != run_id:
        raise ContractError("execution_plan", "PLAN_BINDING_MISMATCH")
    if value.get("host") not in known_hosts():
        raise ContractError("execution_plan.host", "HOST_UNSUPPORTED")
    plan_digest = digest(value.get("plan_digest"), "execution_plan.plan_digest")
    unsigned = dict(value)
    unsigned.pop("plan_digest", None)
    if sha256_bytes(canonical_bytes(unsigned)) != plan_digest:
        raise ContractError("execution_plan.plan_digest", "INPUT_DIGEST_MISMATCH")
    attachment = value.get("helper_allowance")
    if not isinstance(attachment, dict):
        raise ContractError("execution_plan.helper_allowance", "MISSING_FIELD")
    require_keys(attachment, {"ref", "sha256"}, {"ref", "sha256"}, "execution_plan.helper_allowance")
    allowance_ref = validate_ref(attachment["ref"], "execution_plan.helper_allowance.ref")
    if not allowance_ref.startswith("repo:") or "#" in allowance_ref or not allowance_ref.endswith(".json"):
        raise ContractError("execution_plan.helper_allowance.ref", "REPOSITORY_REF_REQUIRED")
    digest(attachment["sha256"], "execution_plan.helper_allowance.sha256")
    return dict(value)


def _filesystem_identity(value: Any, field: str) -> Dict[str, Any]:
    if not isinstance(value, dict):
        raise ContractError(field, "INVALID_OBJECT")
    require_keys(value, {"device", "inode", "path"}, {"device", "inode", "path"}, field)
    path = bounded_string(value["path"], field + ".path", 4096)
    if not Path(path).is_absolute():
        raise ContractError(field + ".path", "ABSOLUTE_PATH_REQUIRED")
    return {
        "device": _integer(value["device"], field + ".device", maximum=2 ** 63 - 1),
        "inode": _integer(value["inode"], field + ".inode", maximum=2 ** 63 - 1),
        "path": path,
    }


def _register_key(
    state_root_identity: Mapping[str, Any], repository_key: str, run_id: str,
    plan_digest: str, allowance_digest: str,
) -> str:
    return sha256_bytes(canonical_bytes({
        "state_root_identity": dict(state_root_identity),
        "repository_digest": repository_key,
        "run_id": run_id,
        "plan_digest": plan_digest,
        "allowance_digest": allowance_digest,
    }))


def _validate_context(value: Any) -> Dict[str, Any]:
    if not isinstance(value, dict):
        raise ContractError("context", "INVALID_OBJECT")
    fields = {
        "schema_version", "register_id", "register_path", "state_root_identity",
        "repository_digest", "run_id", "plan_digest", "allowance_ref",
        "allowance_digest", "allowance_payload_digest", "host_observation_digest",
        "host_observation_payload_digest",
    }
    require_keys(value, fields, fields, "context")
    if value["schema_version"] != 1:
        raise ContractError("context.schema_version", "UNSUPPORTED_SCHEMA")
    register_path = bounded_string(value["register_path"], "context.register_path", 4096)
    if not Path(register_path).is_absolute():
        raise ContractError("context.register_path", "ABSOLUTE_PATH_REQUIRED")
    result = {
        "schema_version": 1,
        "register_id": digest(value["register_id"], "context.register_id"),
        "register_path": register_path,
        "state_root_identity": _filesystem_identity(
            value["state_root_identity"], "context.state_root_identity",
        ),
        "repository_digest": digest(value["repository_digest"], "context.repository_digest"),
        "run_id": opaque(value["run_id"], "context.run_id"),
        "plan_digest": digest(value["plan_digest"], "context.plan_digest"),
        "allowance_ref": validate_ref(value["allowance_ref"], "context.allowance_ref"),
        "allowance_digest": digest(value["allowance_digest"], "context.allowance_digest"),
        "allowance_payload_digest": digest(
            value["allowance_payload_digest"], "context.allowance_payload_digest",
        ),
        "host_observation_digest": digest(value["host_observation_digest"], "context.host_observation_digest"),
        "host_observation_payload_digest": digest(
            value["host_observation_payload_digest"], "context.host_observation_payload_digest",
        ),
    }
    if result["register_id"] != _register_key(
        result["state_root_identity"], result["repository_digest"], result["run_id"],
        result["plan_digest"], result["allowance_digest"],
    ):
        raise ContractError("context.register_id", "REGISTER_BINDING_MISMATCH")
    expected = (
        Path(result["state_root_identity"]["path"]) / "helper-registers"
        / result["register_id"] / "register.json"
    )
    if Path(result["register_path"]) != expected:
        raise ContractError("context.register_path", "REGISTER_BINDING_MISMATCH")
    return result


def _validate_request(value: Any) -> Dict[str, Any]:
    if not isinstance(value, dict):
        raise ContractError("request", "INVALID_OBJECT")
    fields = {
        "schema_version", "request_id", "assignment_id", "parent_role", "helper_role",
        "contract_revision", "model", "reasoning_effort", "scope_refs", "commands",
        "checkpoint_ref", "resource_keys", "budgets",
    }
    require_keys(value, fields, fields, "request")
    budgets = value["budgets"]
    if not isinstance(budgets, dict):
        raise ContractError("request.budgets", "INVALID_OBJECT")
    budget_fields = {"time_seconds", "output_tokens", "file_reads"}
    require_keys(budgets, budget_fields, budget_fields, "request.budgets")
    if value["schema_version"] != 1:
        raise ContractError("request.schema_version", "UNSUPPORTED_SCHEMA")
    return {
        "schema_version": 1,
        "request_id": opaque(value["request_id"], "request.request_id"),
        "assignment_id": opaque(value["assignment_id"], "request.assignment_id"),
        "parent_role": opaque(value["parent_role"], "request.parent_role"),
        "helper_role": opaque(value["helper_role"], "request.helper_role"),
        "contract_revision": _integer(value["contract_revision"], "request.contract_revision", minimum=1),
        "model": bounded_string(value["model"], "request.model", 128),
        "reasoning_effort": opaque(value["reasoning_effort"], "request.reasoning_effort"),
        "scope_refs": _scope_refs(value["scope_refs"], "request.scope_refs"),
        "commands": _commands(value["commands"], "request.commands"),
        "checkpoint_ref": validate_ref(value["checkpoint_ref"], "request.checkpoint_ref", content_required=True),
        "resource_keys": sorted(_unique_ids(value["resource_keys"], "request.resource_keys")),
        "budgets": {
            key: _integer(budgets[key], "request.budgets." + key, minimum=0)
            for key in budget_fields
        },
    }


def _required_amounts(request: Mapping[str, Any]) -> Dict[str, int]:
    return {
        "children": 1, "concurrency": 1, "commands": len(request["commands"]),
        "time_seconds": request["budgets"]["time_seconds"],
        "output_tokens": request["budgets"]["output_tokens"],
        "file_reads": request["budgets"]["file_reads"],
    }


def _assignment_for(allowance: Mapping[str, Any], assignment_id: str) -> Mapping[str, Any]:
    for item in allowance["assignments"]:
        if item["assignment_id"] == assignment_id:
            return item
    raise ContractError("request.assignment_id", "ASSIGNMENT_NOT_APPROVED")


def _verify_checkpoint(register: Mapping[str, Any], request: Mapping[str, Any]) -> None:
    checkpoint = request["checkpoint_ref"]
    if not checkpoint.startswith("repo:"):
        raise ContractError("request.checkpoint_ref", "REPOSITORY_REF_REQUIRED")
    body, marker, expected_digest = checkpoint[5:].partition("#sha256=")
    if not marker:
        raise ContractError("request.checkpoint_ref", "CONTENT_DIGEST_REQUIRED")
    relative = lexical_relative(body, "request.checkpoint_ref")
    approved = False
    for scope in request["scope_refs"]:
        if not scope.startswith("repo:"):
            continue
        scope_path = scope[5:].rstrip("/")
        approved = relative == scope_path or relative.startswith(scope_path + "/")
        if approved:
            break
    if not approved:
        raise ContractError("request.checkpoint_ref", "SCOPE_EXCEEDED")
    repository = Path(register["repository_identity"]["path"])
    snapshot = safe_file_snapshot(repository / relative, [repository], MAX_RECORD_BYTES)
    if snapshot.digest != expected_digest:
        raise ContractError("request.checkpoint_ref", "INPUT_DIGEST_MISMATCH")


def preflight_record(
    register: Mapping[str, Any], request_value: Any,
) -> Tuple[Dict[str, Any], Dict[str, int], List[str]]:
    request = _validate_request(request_value)
    allowance = register["allowance"]
    assignment = _assignment_for(allowance, request["assignment_id"])
    exact_fields = (
        "parent_role", "helper_role", "contract_revision", "model", "reasoning_effort",
    )
    if any(request[key] != assignment[key] for key in exact_fields):
        raise ContractError("request", "HELPER_ASSIGNMENT_MISMATCH")
    if not set(request["scope_refs"]).issubset(assignment["scope_refs"]):
        raise ContractError("request.scope_refs", "SCOPE_EXCEEDED")
    approved_commands = {canonical_bytes(item) for item in assignment["commands"]}
    if any(canonical_bytes(item) not in approved_commands for item in request["commands"]):
        raise ContractError("request.commands", "COMMAND_NOT_APPROVED")
    if request["helper_role"] == "validation_executor" and not request["commands"]:
        raise ContractError("request.commands", "COMMAND_REQUIRED")
    if request["helper_role"] == "evidence_scout" and request["commands"]:
        raise ContractError("request.commands", "COMMAND_FORBIDDEN")
    if request["resource_keys"] != assignment["resource_keys"]:
        raise ContractError("request.resource_keys", "RESOURCE_EXCEEDED")
    _verify_checkpoint(register, request)
    amounts = _required_amounts(request)
    if any(amounts[key] > assignment["limits"][key] for key in LIMIT_KEYS):
        raise ContractError("request.budgets", "ASSIGNMENT_BUDGET_EXCEEDED")

    missing = []
    host_capabilities = register["host_observation"]["capabilities"]
    observed_assignment = next((
        item for item in register["host_observation"]["supported_assignments"]
        if (item["model"], item["reasoning_effort"])
        == (assignment["model"], assignment["reasoning_effort"])
    ), None)
    if (observed_assignment is None or observed_assignment["status"] != "verified"
            or not _trusted_observation_source(observed_assignment["source"])):
        missing.append("model_effort_assignment")
    for capability in assignment["required_host_capabilities"]:
        observation = host_capabilities.get(capability)
        if (observation is None or observation["status"] != "verified"
                or not _trusted_observation_source(observation["source"])):
            missing.append(capability)
    return request, amounts, sorted(missing)


def _private_directory(path: Path) -> None:
    path.mkdir(parents=True, exist_ok=True, mode=0o700)
    ensure_safe_components(path)
    if os.name != "nt":
        os.chmod(path, 0o700)
        info = path.stat()
        if stat.S_IMODE(info.st_mode) != 0o700:
            raise HelperRegisterError("PERMISSION_VERIFICATION_FAILED")


@contextmanager
def _exclusive_lock(path: Path) -> Iterator[None]:
    if path.exists():
        ensure_safe_components(path, path.parent)
        info = path.lstat()
        if not stat.S_ISREG(info.st_mode) or path.is_symlink():
            raise HelperRegisterError("REGISTER_LOCK_UNSAFE")
    flags = os.O_CREAT | os.O_RDWR | getattr(os, "O_BINARY", 0)
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    descriptor = os.open(str(path), flags, 0o600)
    try:
        if not stat.S_ISREG(os.fstat(descriptor).st_mode):
            raise HelperRegisterError("REGISTER_LOCK_UNSAFE")
        if os.name == "nt":
            import msvcrt
            if os.fstat(descriptor).st_size == 0:
                os.write(descriptor, b"\0")
                os.fsync(descriptor)
            os.lseek(descriptor, 0, os.SEEK_SET)
            msvcrt.locking(descriptor, msvcrt.LK_LOCK, 1)
        else:
            import fcntl
            fcntl.flock(descriptor, fcntl.LOCK_EX)
        yield
    finally:
        if os.name == "nt":
            import msvcrt
            os.lseek(descriptor, 0, os.SEEK_SET)
            msvcrt.locking(descriptor, msvcrt.LK_UNLCK, 1)
        else:
            import fcntl
            fcntl.flock(descriptor, fcntl.LOCK_UN)
        os.close(descriptor)


class HelperRegister:
    def __init__(self, fault_hook: Optional[Callable[[str], None]] = None):
        self.fault_hook = fault_hook or (lambda _point: None)

    @staticmethod
    def _read(path: Path) -> Dict[str, Any]:
        snapshot = safe_json_snapshot(path, [path.parent], MAX_RECORD_BYTES)
        return HelperRegister._validate_record(snapshot.parsed)

    @staticmethod
    def _validate_record(value: Any) -> Dict[str, Any]:
        if not isinstance(value, dict):
            raise HelperRegisterError("REGISTER_INVALID")
        fields = {
            "schema_version", "context", "host", "repository_identity", "allowance_ref", "allowance",
            "host_observation", "usage", "active_concurrency", "active_resources",
            "assignment_usage", "reservations",
        }
        try:
            require_keys(value, fields, fields, "register")
            if value["schema_version"] != 1:
                raise ContractError("register.schema_version", "UNSUPPORTED_SCHEMA")
            context = _validate_context(value["context"])
            if value["context"] != context:
                raise ContractError("register.context", "REGISTER_BINDING_MISMATCH")
            identity = _filesystem_identity(value["repository_identity"], "repository_identity")
            if value["repository_identity"] != identity:
                raise ContractError("repository_identity", "REGISTER_BINDING_MISMATCH")
            repository_key = repository_digest("\0".join((
                str(identity["device"]), str(identity["inode"]), identity["path"],
            )))
            if repository_key != context["repository_digest"]:
                raise ContractError("repository_identity", "REGISTER_BINDING_MISMATCH")
            allowance_ref = validate_ref(value["allowance_ref"], "allowance_ref")
            if allowance_ref != context["allowance_ref"]:
                raise ContractError("allowance_ref", "REGISTER_BINDING_MISMATCH")
            host = value["host"]
            if host not in known_hosts():
                raise ContractError("register.host", "HOST_UNSUPPORTED")
            allowance = validate_allowance(value["allowance"], context["run_id"], host)
            host_observation = validate_host_observation(value["host_observation"])
            if value["allowance"] != allowance or value["host_observation"] != host_observation:
                raise ContractError("register", "REGISTER_BINDING_MISMATCH")
            if sha256_bytes(canonical_bytes(allowance)) != context["allowance_payload_digest"]:
                raise ContractError("register.allowance", "REGISTER_BINDING_MISMATCH")
            if sha256_bytes(canonical_bytes(host_observation)) != context["host_observation_payload_digest"]:
                raise ContractError("register.host_observation", "REGISTER_BINDING_MISMATCH")
            usage = value["usage"]
            require_keys(usage, LIMIT_KEYS, LIMIT_KEYS, "register.usage")
            for key in LIMIT_KEYS:
                _integer(usage[key], "register.usage." + key)
            _integer(value["active_concurrency"], "register.active_concurrency")
            resources = {item["key"]: item["capacity"] for item in allowance["resources"]}
            active_resources = value["active_resources"]
            require_keys(active_resources, resources, resources, "register.active_resources")
            for key, amount in active_resources.items():
                if _integer(amount, "register.active_resources." + key) > resources[key]:
                    raise ContractError("register.active_resources", "RESOURCE_CAPACITY_EXCEEDED")
            assignments = {item["assignment_id"]: item for item in allowance["assignments"]}
            assignment_usage = value["assignment_usage"]
            require_keys(assignment_usage, assignments, assignments, "register.assignment_usage")
            for assignment_id, amounts in assignment_usage.items():
                require_keys(amounts, LIMIT_KEYS, LIMIT_KEYS, "register.assignment_usage")
                for key in LIMIT_KEYS:
                    _integer(amounts[key], "register.assignment_usage." + key)
            reservations = value["reservations"]
            if not isinstance(reservations, dict) or len(reservations) > 4096:
                raise ContractError("register.reservations", "INVALID_OBJECT")
            calculated_usage = {key: 0 for key in LIMIT_KEYS}
            calculated_active = 0
            calculated_resources = {key: 0 for key in resources}
            calculated_assignments = {
                assignment_id: {key: 0 for key in LIMIT_KEYS}
                for assignment_id in assignments
            }
            for request_id, reservation in reservations.items():
                opaque(request_id, "register.request_id")
                if not isinstance(reservation, dict):
                    raise ContractError("register.reservations", "INVALID_OBJECT")
                base_fields = {
                    "request_digest", "assignment_id", "status", "reservation",
                    "resource_keys", "settlement",
                }
                require_keys(
                    reservation, base_fields,
                    base_fields | {"settlement_digest"}, "register.reservations",
                )
                digest(reservation["request_digest"], "register.request_digest")
                assignment_id = opaque(reservation["assignment_id"], "register.assignment_id")
                if assignment_id not in assignments:
                    raise ContractError("register.assignment_id", "ASSIGNMENT_NOT_APPROVED")
                amounts = reservation["reservation"]
                require_keys(amounts, LIMIT_KEYS, LIMIT_KEYS, "register.reservation")
                for key in LIMIT_KEYS:
                    amount = _integer(amounts[key], "register.reservation." + key)
                    if key != "concurrency":
                        calculated_usage[key] += amount
                    calculated_assignments[assignment_id][key] += amount
                keys = _unique_ids(reservation["resource_keys"], "register.resource_keys")
                if not set(keys).issubset(resources):
                    raise ContractError("register.resource_keys", "UNKNOWN_RESOURCE")
                if reservation["status"] == "active":
                    if reservation["settlement"] is not None or "settlement_digest" in reservation:
                        raise ContractError("register.reservations", "SETTLEMENT_CONFLICT")
                    calculated_active += 1
                    for key in keys:
                        calculated_resources[key] += 1
                else:
                    if reservation["status"] not in TERMINAL_STATES or reservation["settlement"] is None:
                        raise ContractError("register.reservations", "INVALID_TERMINAL_STATE")
                    digest(reservation.get("settlement_digest"), "register.settlement_digest")
                    calculated_assignments[assignment_id]["concurrency"] -= amounts["concurrency"]
            if calculated_usage != usage or calculated_active != value["active_concurrency"]:
                raise ContractError("register", "REGISTER_ACCOUNTING_MISMATCH")
            if calculated_resources != active_resources or calculated_assignments != assignment_usage:
                raise ContractError("register", "REGISTER_ACCOUNTING_MISMATCH")
        except ContractError as error:
            raise HelperRegisterError("REGISTER_INVALID") from error
        return value

    def _write(self, path: Path, value: Mapping[str, Any]) -> None:
        payload = canonical_bytes(value)
        if len(payload) > MAX_RECORD_BYTES:
            raise HelperRegisterError("REGISTER_TOO_LARGE")
        descriptor, temporary = tempfile.mkstemp(prefix=".register.", suffix=".tmp", dir=str(path.parent))
        try:
            if os.name != "nt":
                os.fchmod(descriptor, 0o600)
            offset = 0
            while offset < len(payload):
                written = os.write(descriptor, payload[offset:])
                if written <= 0:
                    raise OSError("short register write")
                offset += written
            os.fsync(descriptor)
            self.fault_hook("before_replace")
            os.close(descriptor)
            descriptor = -1
            os.replace(temporary, path)
            if os.name != "nt":
                directory = os.open(str(path.parent), os.O_RDONLY)
                try:
                    os.fsync(directory)
                finally:
                    os.close(directory)
        finally:
            if descriptor >= 0:
                os.close(descriptor)

    def initialize(
        self, state_root: Path, repo: Path, run_id: str, plan_path: Path,
        allowance_path: Path, host_observation_path: Path,
    ) -> Dict[str, Any]:
        if not state_root.is_absolute():
            raise HelperRegisterError("STATE_ROOT_MUST_BE_ABSOLUTE")
        run = opaque(run_id, "run_id")
        root = state_root.absolute()
        _private_directory(root)
        local_filesystem_identity(root)
        root = root.resolve(strict=True)
        root_info = root.stat()
        root_identity = {
            "device": int(root_info.st_dev), "inode": int(root_info.st_ino), "path": str(root),
        }
        repository = repo.resolve(strict=True)
        device, inode, display = repository_identity(repository)
        repository_key = repository_digest("\0".join((str(device), str(inode), display)))
        plan_snapshot = safe_json_snapshot(plan_path, [plan_path.parent], MAX_RECORD_BYTES)
        plan = _validate_plan(plan_snapshot.parsed, run)
        attachment = plan["helper_allowance"]
        expected_allowance_path = repository / attachment["ref"][5:]
        if allowance_path.resolve(strict=True) != expected_allowance_path.resolve(strict=True):
            raise ContractError("allowance", "ALLOWANCE_REF_MISMATCH")
        allowance_snapshot = safe_json_snapshot(allowance_path, [repository], MAX_RECORD_BYTES)
        if allowance_snapshot.digest != attachment["sha256"]:
            raise ContractError("allowance", "INPUT_DIGEST_MISMATCH")
        allowance = validate_allowance(allowance_snapshot.parsed, run, plan["host"])
        host_snapshot = safe_json_snapshot(
            host_observation_path, [host_observation_path.parent], MAX_RECORD_BYTES,
        )
        host = validate_host_observation(host_snapshot.parsed)
        register_id = _register_key(
            root_identity, repository_key, run, plan["plan_digest"], allowance_snapshot.digest,
        )
        _private_directory(root / "helper-registers")
        directory = root / "helper-registers" / register_id
        _private_directory(directory)
        path = directory / "register.json"
        lock = directory / "register.lock"
        context = {
            "schema_version": 1, "register_id": register_id, "register_path": str(path),
            "state_root_identity": root_identity, "repository_digest": repository_key,
            "run_id": run, "plan_digest": plan["plan_digest"],
            "allowance_ref": attachment["ref"], "allowance_digest": allowance_snapshot.digest,
            "allowance_payload_digest": sha256_bytes(canonical_bytes(allowance)),
            "host_observation_digest": host_snapshot.digest,
            "host_observation_payload_digest": sha256_bytes(canonical_bytes(host)),
        }
        record = {
            "schema_version": 1, "context": context, "host": plan["host"],
            "repository_identity": {"device": device, "inode": inode, "path": display},
            "allowance_ref": attachment["ref"], "allowance": allowance,
            "host_observation": host,
            "usage": {key: 0 for key in LIMIT_KEYS},
            "active_concurrency": 0,
            "active_resources": {item["key"]: 0 for item in allowance["resources"]},
            "assignment_usage": {
                item["assignment_id"]: {key: 0 for key in LIMIT_KEYS}
                for item in allowance["assignments"]
            },
            "reservations": {},
        }
        with _exclusive_lock(lock):
            if path.exists():
                existing = self._read(path)
                immutable = (
                    "context", "host", "repository_identity", "allowance_ref", "allowance",
                    "host_observation",
                )
                if any(existing.get(key) != record[key] for key in immutable):
                    raise HelperRegisterError("INITIALIZATION_CONFLICT")
                code = "REPLAYED"
            else:
                self._write(path, record)
                code = "INITIALIZED"
        return {"ok": True, "code": code, "register_path": str(path), "context": context}

    @staticmethod
    def _bound_path(register_path: Path, context_value: Any) -> Tuple[Path, Dict[str, Any]]:
        context = _validate_context(context_value)
        if register_path.is_symlink():
            raise HelperRegisterError("REGISTER_BINDING_MISMATCH")
        path = register_path.resolve(strict=True)
        if path != Path(context["register_path"]):
            raise HelperRegisterError("REGISTER_BINDING_MISMATCH")
        root_identity = context["state_root_identity"]
        try:
            root_device, root_inode, root_path = repository_identity(Path(root_identity["path"]))
        except (ContractError, OSError) as error:
            raise HelperRegisterError("REGISTER_BINDING_MISMATCH") from error
        if (root_device, root_inode, root_path) != (
            root_identity["device"], root_identity["inode"], root_identity["path"],
        ):
            raise HelperRegisterError("REGISTER_BINDING_MISMATCH")
        ensure_safe_components(path, Path(root_path))
        return path, context

    def _bound_record(self, register_path: Path, context_value: Any) -> Tuple[Dict[str, Any], Dict[str, Any]]:
        path, context = self._bound_path(register_path, context_value)
        record = self._read(path)
        if record.get("context") != context:
            raise HelperRegisterError("REGISTER_BINDING_MISMATCH")
        identity = record["repository_identity"]
        try:
            current_repository = repository_identity(Path(identity["path"]))
        except (ContractError, OSError) as error:
            raise HelperRegisterError("REGISTER_BINDING_MISMATCH") from error
        if current_repository != (identity["device"], identity["inode"], identity["path"]):
            raise HelperRegisterError("REGISTER_BINDING_MISMATCH")
        repository = Path(identity["path"])
        allowance_relative = lexical_relative(context["allowance_ref"][5:], "context.allowance_ref")
        try:
            allowance_snapshot = safe_json_snapshot(
                repository / allowance_relative, [repository], MAX_RECORD_BYTES,
            )
            current_allowance = validate_allowance(
                allowance_snapshot.parsed, context["run_id"], record["host"],
            )
        except (ContractError, OSError) as error:
            raise HelperRegisterError("REGISTER_BINDING_MISMATCH") from error
        if (allowance_snapshot.digest != context["allowance_digest"]
                or current_allowance != record["allowance"]):
            raise HelperRegisterError("REGISTER_BINDING_MISMATCH")
        return record, context

    def preflight(self, register_path: Path, context: Any, request: Any) -> Dict[str, Any]:
        record, _ = self._bound_record(register_path, context)
        normalized, amounts, missing = preflight_record(record, request)
        if missing:
            return {
                "ok": False, "code": "BLOCKED_UNSUPPORTED", "request_id": normalized["request_id"],
                "missing_host_capabilities": missing,
            }
        return {"ok": True, "code": "PREFLIGHT_READY", "request_id": normalized["request_id"], "reservation": amounts}

    def reserve(self, register_path: Path, context: Any, request: Any) -> Dict[str, Any]:
        path, normalized_context = self._bound_path(register_path, context)
        lock = path.parent / "register.lock"
        with _exclusive_lock(lock):
            record, _ = self._bound_record(path, normalized_context)
            normalized = _validate_request(request)
            request_digest = sha256_bytes(canonical_bytes(normalized))
            existing = record["reservations"].get(normalized["request_id"])
            if existing is not None:
                if existing["request_digest"] != request_digest:
                    raise HelperRegisterError("RESERVATION_CONFLICT")
                return {
                    "ok": True, "code": "REPLAYED", "request_id": normalized["request_id"],
                    "request_digest": request_digest, "status": existing["status"],
                    "new_dispatch_authorized": False,
                }
            normalized, amounts, missing = preflight_record(record, normalized)
            if missing:
                return {
                    "ok": False, "code": "BLOCKED_UNSUPPORTED", "request_id": normalized["request_id"],
                    "missing_host_capabilities": missing,
                }
            assignment_usage = record["assignment_usage"][normalized["assignment_id"]]
            assignment = _assignment_for(record["allowance"], normalized["assignment_id"])
            for key in LIMIT_KEYS:
                active = record["active_concurrency"] if key == "concurrency" else record["usage"][key]
                if active + amounts[key] > record["allowance"]["shared_limits"][key]:
                    raise HelperRegisterError("SHARED_LIMIT_EXCEEDED")
                assignment_active = assignment_usage[key]
                if assignment_active + amounts[key] > assignment["limits"][key]:
                    raise HelperRegisterError("ASSIGNMENT_LIMIT_EXCEEDED")
            capacities = {item["key"]: item["capacity"] for item in record["allowance"]["resources"]}
            for key in normalized["resource_keys"]:
                if record["active_resources"][key] + 1 > capacities[key]:
                    raise HelperRegisterError("RESOURCE_CAPACITY_EXCEEDED")
            for key in LIMIT_KEYS:
                if key != "concurrency":
                    record["usage"][key] += amounts[key]
                assignment_usage[key] += amounts[key]
            record["active_concurrency"] += 1
            for key in normalized["resource_keys"]:
                record["active_resources"][key] += 1
            record["reservations"][normalized["request_id"]] = {
                "request_digest": request_digest, "assignment_id": normalized["assignment_id"],
                "status": "active", "reservation": amounts,
                "resource_keys": normalized["resource_keys"], "settlement": None,
            }
            self._write(path, record)
        return {
            "ok": True, "code": "RESERVED", "request_id": normalized["request_id"],
            "request_digest": request_digest, "status": "active", "new_dispatch_authorized": True,
        }

    def settle(self, register_path: Path, context: Any, value: Any) -> Dict[str, Any]:
        if not isinstance(value, dict):
            raise ContractError("settlement", "INVALID_OBJECT")
        fields = {
            "schema_version", "request_id", "request_digest", "terminal_state",
            "evidence_refs", "uncertainty", "session_identity_sha256", "usage_ref",
        }
        require_keys(value, fields, fields, "settlement")
        if value["schema_version"] != 1 or value["terminal_state"] not in TERMINAL_STATES:
            raise ContractError("settlement", "INVALID_TERMINAL_STATE")
        evidence = value["evidence_refs"]
        if not isinstance(evidence, list) or len(evidence) > 64:
            raise ContractError("settlement.evidence_refs", "INVALID_LIST")
        normalized_evidence = sorted(validate_ref(item, "settlement.evidence_refs") for item in evidence)
        if len(normalized_evidence) != len(set(normalized_evidence)):
            raise ContractError("settlement.evidence_refs", "DUPLICATE_VALUE")
        settlement = {
            "schema_version": 1,
            "request_id": opaque(value["request_id"], "settlement.request_id"),
            "request_digest": digest(value["request_digest"], "settlement.request_digest"),
            "terminal_state": value["terminal_state"],
            "evidence_refs": normalized_evidence,
            "uncertainty": bounded_string(value["uncertainty"], "settlement.uncertainty", 2048),
            "session_identity_sha256": digest(value["session_identity_sha256"], "settlement.session_identity_sha256"),
            "usage_ref": None if value["usage_ref"] is None else validate_ref(value["usage_ref"], "settlement.usage_ref"),
        }
        settlement_digest = sha256_bytes(canonical_bytes(settlement))
        path, normalized_context = self._bound_path(register_path, context)
        lock = path.parent / "register.lock"
        with _exclusive_lock(lock):
            record, _ = self._bound_record(path, normalized_context)
            reservation = record["reservations"].get(settlement["request_id"])
            if reservation is None or reservation["request_digest"] != settlement["request_digest"]:
                raise HelperRegisterError("RESERVATION_NOT_FOUND")
            if reservation["settlement"] is not None:
                if reservation["settlement_digest"] != settlement_digest:
                    raise HelperRegisterError("SETTLEMENT_CONFLICT")
                return {"ok": True, "code": "REPLAYED", "request_id": settlement["request_id"], "status": reservation["status"]}
            if reservation["status"] != "active":
                raise HelperRegisterError("SETTLEMENT_CONFLICT")
            reservation["status"] = settlement["terminal_state"]
            reservation["settlement"] = settlement
            reservation["settlement_digest"] = settlement_digest
            record["active_concurrency"] -= 1
            assignment_usage = record["assignment_usage"][reservation["assignment_id"]]
            assignment_usage["concurrency"] -= 1
            for key in reservation["resource_keys"]:
                record["active_resources"][key] -= 1
            self._write(path, record)
        return {"ok": True, "code": "SETTLED", "request_id": settlement["request_id"], "status": settlement["terminal_state"]}

    def status(self, register_path: Path, context: Any) -> Dict[str, Any]:
        record, normalized_context = self._bound_record(register_path, context)
        return {
            "ok": True, "code": "STATUS", "context": normalized_context,
            "usage": record["usage"], "active_concurrency": record["active_concurrency"],
            "active_resources": record["active_resources"],
            "reservations": {
                key: {"status": item["status"], "assignment_id": item["assignment_id"]}
                for key, item in sorted(record["reservations"].items())
            },
            "evidence_notice": "trusted-caller evidence; not proof of host confinement or authenticity",
        }
