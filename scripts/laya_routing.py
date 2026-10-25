"""Shared, conservative policy for resident Laya work-packet selection."""

import math

PROFILES = {
    "file_finding": ("gpt-6-luna", "low", "explorer"),
    "bounded_reading": ("gpt-6-luna", "medium", "explorer"),
    "coding": ("gpt-6.1-sol", "high", "worker"),
    "complex": ("gpt-6-astra", "high", "default"),
}
QUESTIONS = {"route": {
    "type": "choice",
    "instructions": "Classify the work required by this task. Choose complex for difficult reasoning or high-risk changes.",
    "criteria": {
        "file_finding": "Locate file names, paths, symbols or text matches. Return locations only; no code changes.",
        "bounded_reading": "Read a few specified files and summarize existing behavior. No changes or difficult diagnosis.",
        "coding": "Implement a clear feature, routine bug fix, refactor or tests with known requirements.",
        "complex": "Architecture decisions, ambiguous debugging, concurrency, security or changes across subsystem boundaries.",
    },
}}


def spawn_settings(route):
    model, effort, agent_type = PROFILES[route]
    return {"model": model, "reasoning_effort": effort,
            "agent_type": agent_type, "fork_turns": "none"}


def select_prediction(prediction):
    if not isinstance(prediction, dict):
        return {"status": "fallback", "reason": "invalid_prediction"}
    usage = prediction.get("usage")
    if not isinstance(usage, dict) or usage.get("truncated") is not False:
        return {"status": "fallback", "reason": "truncated"}
    answers = prediction.get("answers")
    answer = answers.get("route") if isinstance(answers, dict) else None
    if not isinstance(answer, dict):
        return {"status": "fallback", "reason": "invalid_prediction"}
    route, scores = answer.get("choice"), answer.get("probabilities")
    if not isinstance(route, str) or route not in PROFILES or not isinstance(scores, dict):
        return {"status": "fallback", "reason": "invalid_prediction"}
    if set(scores) != set(PROFILES) or not all(
        type(value) in (int, float) and math.isfinite(value) and 0 <= value <= 1
        for value in scores.values()
    ):
        return {"status": "fallback", "reason": "invalid_prediction"}
    runner_up = max(value for name, value in scores.items() if name != route)
    if scores[route] < 0.60 or scores[route] < runner_up + 0.15:
        return {"status": "fallback", "reason": "uncertain", "route": route}
    return {"status": "selected", "route": route, "spawn": spawn_settings(route)}


def validate_selection(result):
    """Accept only local policy settings, never arbitrary server-supplied models."""
    if not isinstance(result, dict):
        return {"status": "fallback", "reason": "invalid_response"}
    if result.get("status") == "fallback":
        reason = result.get("reason")
        if isinstance(reason, str) and reason in {"uncertain", "truncated", "invalid_prediction", "disabled", "unavailable"}:
            return {"status": "fallback", "reason": reason}
    route = result.get("route")
    if result.get("status") == "selected" and isinstance(route, str) and route in PROFILES:
        expected = spawn_settings(route)
        if result.get("spawn") == expected:
            return {"status": "selected", "route": route, "spawn": expected}
    return {"status": "fallback", "reason": "invalid_response"}
