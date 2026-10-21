"""Classify one bounded work packet and return explicit Codex spawn settings."""

import json
import subprocess
import sys
import time
import uuid
from urllib.error import URLError

import hook

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


def select_subagent(result):
    if result["status"] != "ok":
        return {"status": "fallback", "reason": result["status"]}
    route = result["route"]
    scores = result["probabilities"]
    runner_up = max(score for name, score in scores.items() if name != route)
    if scores[route] < 0.60 or scores[route] - runner_up < 0.15:
        return {"status": "fallback", "reason": "uncertain", "route": route}
    model, effort, agent_type = PROFILES[route]
    return {
        "status": "selected", "route": route,
        "spawn": {"model": model, "reasoning_effort": effort,
                  "agent_type": agent_type, "fork_turns": "none"},
    }


def classify(task, config):
    try:
        return hook.request_route(task, config, QUESTIONS)
    except URLError as error:
        # Windows can time out before reporting a refused loopback connection.
        # The manager checks process ownership and the port before starting.
        if not isinstance(error.reason, (ConnectionRefusedError, TimeoutError)):
            raise
    subprocess.run([
        config["powershell"], "-NoProfile", "-NonInteractive", "-File",
        str(hook.ROOT / "manage.ps1"), "Start",
    ], check=True, timeout=10, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    deadline = time.monotonic() + 20
    while True:
        try:
            return hook.request_route(task, config, QUESTIONS)
        except (URLError, TimeoutError):
            if time.monotonic() >= deadline:
                raise TimeoutError("Laya startup deadline exceeded") from None
            time.sleep(0.5)


def main():
    started = time.perf_counter()
    record = {"event": "SubagentSelection", "decision_id": uuid.uuid4().hex}
    try:
        config = json.loads((hook.ROOT / "config.json").read_text(encoding="utf-8-sig"))
        if not config.get("subagent_routing_enabled", False):
            selection = {"status": "fallback", "reason": "disabled"}
        else:
            raw = sys.stdin.buffer.read(16385)
            if len(raw) > 16384:
                raise ValueError("Task packet too large")
            packet = json.loads(raw.decode("utf-8-sig"))
            task = packet.get("task")
            if not isinstance(task, str) or not task.strip() or len(task) > 4000:
                raise ValueError("Expected a concise task")
            result = classify(task, config)
            record.update(result)
            selection = select_subagent(result)
    except Exception as error:
        selection = {"status": "fallback", "reason": "unavailable", "error_type": type(error).__name__}
    selection["decision_id"] = record["decision_id"]
    record.update(selection)
    record["elapsed_ms"] = round((time.perf_counter() - started) * 1000, 2)
    # Selection does not prove execution; only the subsequent spawn can do that.
    record["executed"] = False
    hook.append_record(record)
    print(json.dumps(selection))


if __name__ == "__main__":
    main()
