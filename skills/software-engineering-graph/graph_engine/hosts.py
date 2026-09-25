"""Host expansions for intelligence-class assignments.

The size matrix assigns each role an intelligence class and requested effort.
This table maps that pair onto a concrete vendor model for the selected host.

New runs default to the Astra catalog. The explicit Codex catalog recommends
Sol for hosts without Astra access. Historical catalog defaults remain frozen.
"""

from typing import Dict, Optional, Tuple


INTELLIGENCE_CLASSES = ("economy", "reasoning", "primary-thread")
DEFAULT_HOST = "codex-astra"
LEGACY_HOST = "codex"
CURRENT_CATALOG_REVISIONS = {"claude": 3, "codex": 5, "codex-astra": 5, "cursor": 3}
SUPPORTED_CATALOG_REVISIONS = {"claude": (1, 3), "codex": (2, 3, 4, 5), "codex-astra": (2, 3, 4, 5), "cursor": (2, 3)}
REASONING_DISPATCH_WEIGHTS = {"high": 3, "xhigh": 4, "max": 5}
MODEL_DISPATCH_WEIGHTS = {
    ("gpt-6-astra", "medium"): 3,
    ("gpt-6-luna", "max"): 3,
    ("gpt-6-sol", "high"): 3,
    ("gpt-6-sol", "xhigh"): 4,
    ("gpt-6-sol", "max"): 5,
}

# (intelligence_class, requested_effort) -> (model, reasoning_effort, dispatch_model)
HOST_MATRIX: Dict[str, Dict[Tuple[str, str], Tuple[str, str, str]]] = {
    "claude": {
        # Haiku 4.5 does not expose effort. Use Sonnet at low effort for
        # economical branches so every approved assignment remains exact.
        ("economy", "max"): ("claude-sonnet-5", "low", "claude-sonnet-5"),
        ("reasoning", "medium"): ("claude-opus-5", "medium", "claude-opus-5"),
        ("reasoning", "high"): ("claude-opus-5", "high", "claude-opus-5"),
        ("reasoning", "xhigh"): ("claude-opus-5", "xhigh", "claude-opus-5"),
        ("reasoning", "max"): ("claude-opus-5", "max", "claude-opus-5"),
        ("primary-thread", "inherited"): ("primary-thread", "inherited", "primary-thread"),
    },
    "codex": {
        ("economy", "max"): ("gpt-5.6-luna", "max", "gpt-5.6-luna"),
        ("reasoning", "medium"): ("gpt-5.6-sol", "medium", "gpt-5.6-sol"),
        ("reasoning", "high"): ("gpt-5.6-sol", "high", "gpt-5.6-sol"),
        ("reasoning", "xhigh"): ("gpt-5.6-sol", "xhigh", "gpt-5.6-sol"),
        ("reasoning", "max"): ("gpt-5.6-sol", "max", "gpt-5.6-sol"),
        ("primary-thread", "inherited"): ("primary-thread", "inherited", "primary-thread"),
    },
    "codex-astra": {
        ("economy", "max"): ("gpt-5.6-luna", "max", "gpt-5.6-luna"),
        ("reasoning", "low"): ("gpt-6-astra", "low", "gpt-6-astra"),
        ("reasoning", "medium"): ("gpt-6-astra", "medium", "gpt-6-astra"),
        ("reasoning", "high"): ("gpt-6-astra", "high", "gpt-6-astra"),
        ("reasoning", "xhigh"): ("gpt-6-astra", "xhigh", "gpt-6-astra"),
        ("reasoning", "max"): ("gpt-6-astra", "max", "gpt-6-astra"),
        ("primary-thread", "inherited"): ("primary-thread", "inherited", "primary-thread"),
    },
    "cursor": {
        ("economy", "max"): ("composer-2.5", "high", "composer-2.5"),
        ("reasoning", "medium"): ("cursor-grok-4.6", "medium", "cursor-grok-4.6-medium"),
        ("reasoning", "high"): ("cursor-grok-4.6", "high", "cursor-grok-4.6-high"),
        ("reasoning", "xhigh"): ("cursor-grok-4.6", "xhigh", "cursor-grok-4.6-xhigh"),
        ("reasoning", "max"): ("cursor-grok-4.6", "xhigh", "cursor-grok-4.6-xhigh"),
        ("primary-thread", "inherited"): ("primary-thread", "inherited", "primary-thread"),
    },
}

SUPERVISOR_CLASS = {
    "claude": ("reasoning", "xhigh"),
    "codex": ("reasoning", "xhigh"),
    "codex-astra": ("reasoning", "xhigh"),
    "cursor": ("reasoning", "high"),
}
PUBLICATION_CLASS = {
    "claude": ("economy", "max"),
    "codex": ("economy", "max"),
    "codex-astra": ("economy", "max"),
    "cursor": ("economy", "max"),
}

# Defaults and selectable pairs are separate. HOST_MATRIX stays frozen for old approvals.
# These pairs are catalog knowledge, not evidence of account/runtime availability.
MODEL_OPTIONS_V3 = {
    "codex": {
        "gpt-5.6-luna": ("low", "medium", "high", "xhigh", "max"),
        "gpt-5.6-sol": ("low", "medium", "high", "xhigh", "max"),
        "gpt-5.6-terra": ("low", "medium", "high", "xhigh", "max"),
        "gpt-6-astra": ("low", "medium", "high", "xhigh", "max"),
    },
    "claude": {
        "claude-sonnet-5": ("low", "medium", "high", "xhigh", "max"),
        "claude-opus-5": ("low", "medium", "high", "xhigh", "max"),
        "claude-fable-5-1": ("low", "medium", "high", "xhigh", "max"),
    },
    "cursor": {
        "gemini-3.8-flash": ("low", "medium", "high"),
        "grok-4.7": ("low", "medium", "high", "xhigh"),
        "composer-2.5": ("high",),
        "claude-sonnet-5": ("low", "medium", "high", "xhigh", "max"),
        "claude-opus-5": ("low", "medium", "high", "xhigh", "max"),
        "claude-fable-5-1": ("low", "medium", "high", "xhigh", "max"),
        "gpt-5.6-sol": ("low", "medium", "high", "xhigh", "max"),
    },
}
MODEL_OPTIONS_V3["codex-astra"] = MODEL_OPTIONS_V3["codex"]
MODEL_OPTIONS = dict(MODEL_OPTIONS_V3)
MODEL_OPTIONS["codex"] = {
    **MODEL_OPTIONS_V3["codex"],
    "gpt-6-luna": ("low", "medium", "high", "xhigh", "max"),
    "gpt-6-sol": ("low", "medium", "high", "xhigh", "max"),
}
MODEL_OPTIONS["codex-astra"] = MODEL_OPTIONS["codex"]


def selected_dispatch_model(host: str, model: str, effort: str, revision: Optional[int] = None) -> str:
    """Resolve a human selection without treating recommendations as requirements."""
    catalog_for(host)
    if effort in model_options(host, revision).get(model, ()):
        return model
    return dispatch_model(host, model, effort)


def recommended_assignment(host: str, workload: str, revision: Optional[int] = None) -> Tuple[str, str]:
    """Catalog suggestions; actual assignments remain subject to human approval."""
    catalog_for(host)
    if workload == "helper":
        return {"claude": ("claude-sonnet-5", "low"),
                "cursor": ("gemini-3.8-flash", "low")}.get(
                    host, ("gpt-5.6-luna" if revision == 3 else "gpt-6-luna",
                           "low" if revision in {3, 4} else "max"))
    model = {"claude": "claude-opus-5", "cursor": "grok-4.7",
             "codex": "gpt-5.6-sol" if revision == 3 else "gpt-6-sol",
             "codex-astra": "gpt-6-astra"}[host]
    return model, "high" if workload == "review" else "medium"


def model_options(host: str, revision: Optional[int] = None) -> Dict[str, Tuple[str, ...]]:
    catalog_for(host)
    return dict((MODEL_OPTIONS_V3 if revision == 3 else MODEL_OPTIONS)[host])


def known_hosts() -> Tuple[str, ...]:
    return tuple(sorted(HOST_MATRIX))


def catalog_for(host: str) -> Dict[Tuple[str, str], Tuple[str, str, str]]:
    catalog = HOST_MATRIX.get(host)
    if catalog is None:
        raise ValueError("HOST_UNSUPPORTED")
    return catalog


def resolve_row(host: str, intelligence_class: str, requested_effort: str) -> Tuple[str, str, str]:
    row = catalog_for(host).get((intelligence_class, requested_effort))
    if row is None:
        raise ValueError("MODEL_ASSIGNMENT_INVALID")
    return row


def resolve_assignment(host: str, intelligence_class: str, requested_effort: str) -> Tuple[str, str]:
    model, effort, _dispatch = resolve_row(host, intelligence_class, requested_effort)
    return model, effort


def dispatch_model(host: str, model: str, effort: str) -> str:
    for row in catalog_for(host).values():
        if row[0] == model and row[1] == effort:
            return row[2]
    raise ValueError("MODEL_ASSIGNMENT_INVALID")


def classify(host: str, model: str) -> Optional[str]:
    for (intelligence_class, _requested), row in catalog_for(host).items():
        if row[0] == model:
            return intelligence_class
    return None


def economy_effort(host: str) -> str:
    for (intelligence_class, _requested), row in catalog_for(host).items():
        if intelligence_class == "economy":
            return row[1]
    raise ValueError("MODEL_ASSIGNMENT_INVALID")


def supervisor_recommendation(host: str) -> Tuple[str, str, str]:
    catalog_for(host)
    return resolve_row(host, *SUPERVISOR_CLASS[host])


def publication_assignment(host: str) -> Tuple[str, str, str]:
    catalog_for(host)
    return resolve_row(host, *PUBLICATION_CLASS[host])


def dispatch_weight_for(model: str, effort: str) -> Optional[int]:
    """Return the engine delegation weight for a concrete host model pair."""
    if (model, effort) in MODEL_DISPATCH_WEIGHTS:
        return MODEL_DISPATCH_WEIGHTS[(model, effort)]
    if model == "primary-thread":
        return None
    for host in HOST_MATRIX:
        intelligence_class = classify(host, model)
        if intelligence_class is None:
            continue
        if intelligence_class == "economy":
            return 3 if effort == economy_effort(host) else None
        if intelligence_class == "reasoning":
            return REASONING_DISPATCH_WEIGHTS.get(effort)
    return None


def supported_dispatch_weights() -> Dict[Tuple[str, str], int]:
    weights: Dict[Tuple[str, str], int] = dict(MODEL_DISPATCH_WEIGHTS)
    for catalog in HOST_MATRIX.values():
        for (intelligence_class, _requested), row in catalog.items():
            if intelligence_class == "economy":
                weights[(row[0], row[1])] = 3
            elif intelligence_class == "reasoning":
                weight = dispatch_weight_for(row[0], row[1])
                if weight is not None:
                    weights[(row[0], row[1])] = weight
    return weights
