"""Pure assessment of a supplied PR evidence snapshot; grants no external authority."""

import re
from typing import Any, Dict, Mapping, Sequence


def _valid_commit_sha(value: str) -> bool:
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", value) is not None


def assess_merge_readiness(
    *, head_sha: str, reviewed_sha: str, ci_head_sha: str,
    required_check_ids: Sequence[str], checks: Mapping[str, str],
    review_decision: str, unresolved_review_count: int, graph_status: str,
    dry_run: bool = True,
) -> Dict[str, Any]:
    """Require exact-head review and complete CI before reporting dry-run readiness.

    The caller must obtain an authoritative, fresh snapshot. This function does
    not fetch evidence or turn caller-supplied values into trusted receipts.
    """
    blockers = []
    if not all(_valid_commit_sha(value) for value in (head_sha, reviewed_sha, ci_head_sha)):
        blockers.append("INVALID_HEAD_EVIDENCE")
    if head_sha != reviewed_sha:
        blockers.append("REVIEW_HEAD_CHANGED")
    if head_sha != ci_head_sha:
        blockers.append("CI_HEAD_CHANGED")
    if graph_status != "complete":
        blockers.append("GRAPH_INCOMPLETE")
    if review_decision != "APPROVED":
        blockers.append("REVIEW_NOT_APPROVED")
    if (isinstance(unresolved_review_count, bool) or not isinstance(unresolved_review_count, int)
            or unresolved_review_count != 0):
        blockers.append("UNRESOLVED_REVIEWS")
    if (isinstance(required_check_ids, (str, bytes)) or not required_check_ids
            or any(not isinstance(item, str) or not item for item in required_check_ids)
            or len(set(required_check_ids)) != len(required_check_ids)):
        blockers.append("REQUIRED_CHECKS_UNSPECIFIED")
    else:
        for check_id in sorted(required_check_ids):
            outcome = checks.get(check_id)
            if outcome != "success":
                blockers.append("CHECK_{}:{}".format("MISSING" if outcome is None else "NOT_PASSING", check_id))
    if dry_run is not True:
        blockers.append("LIVE_EXECUTION_UNSUPPORTED")
    return {
        "decision": "BLOCKED" if blockers else "DRY_RUN_READY",
        "head_sha": head_sha, "blockers": blockers,
        "merge_authorized": False, "deploy_authorized": False,
    }
