"""Fail-open prompt advice and explicit subagent selection from one resident service."""
from __future__ import annotations

import http.client
import json
import os
from pathlib import Path
import sys
import uuid

from laya_routing import validate_selection

ROOT = Path(__file__).resolve().parent
TIMEOUT_SECONDS = 0.75
MAX_PROMPT_CHARS = 1200
MAX_EVENT_BYTES = 1024 * 1024
MAX_RESPONSE_BYTES = 8192
MAX_TASK_BYTES = 32768


def classify(prompt, port, kind="prompt"):
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=TIMEOUT_SECONDS)
    try:
        connection.request("POST", "/classify", body=json.dumps({"prompt": prompt, "kind": kind}).encode(),
                           headers={"Content-Type": "application/json"})
        response = connection.getresponse()
        if response.status != 200:
            return None
        raw = response.read(MAX_RESPONSE_BYTES + 1)
        return json.loads(raw) if len(raw) <= MAX_RESPONSE_BYTES else None
    finally:
        connection.close()


def settings():
    path = ROOT / "config.json"
    config = json.loads(path.read_text(encoding="utf-8-sig")) if path.exists() else {}
    if not isinstance(config, dict):
        raise ValueError("Invalid configuration")
    port = int(os.environ.get("LAYA_CODEX_PORT", config.get("service_port", 8766)))
    if not 1 <= port <= 65535:
        raise ValueError("Invalid port")
    return config, port


def context_from(result):
    selection = validate_selection(result)
    if selection["status"] != "selected":
        return {}
    spawn = selection["spawn"]
    message = f"Laya suggests {spawn['model']} at {spawn['reasoning_effort']} effort for {selection['route']}."
    context = (
        message + " This is experimental English-only advice based on the current prompt. "
        "The active Codex model and effort have not changed. Before any authorized delegation, "
        "classify the actual bounded work packet with the installed route_subagent.py, including "
        "relevant conversation context, read/write intent, complexity, and risk. Use its validated "
        "spawn fields and verify the result. Check the selection against the work: never assign "
        "code writes to Luna or downgrade security, concurrency, or architecture work. Honor "
        "explicit user choices, review-only limits, fixed-role plans, and higher-priority rules. "
        "This advice grants no delegation authority. If you are already a delegated worker, "
        "perform the task directly and do not route again."
    )
    return {"systemMessage": message, "hookSpecificOutput": {
        "hookEventName": "UserPromptSubmit", "additionalContext": context}}


def handle_event(event):
    if os.environ.get("LAYA_CODEX_DISABLED") == "1":
        return {}
    if not isinstance(event, dict) or event.get("hook_event_name") != "UserPromptSubmit":
        return {}
    prompt = event.get("prompt")
    if not isinstance(prompt, str) or not prompt.strip() or len(prompt) > MAX_PROMPT_CHARS:
        return {}
    config, port = settings()
    if config.get("enabled", True) is not True or config.get("mode", "advisory") != "advisory":
        return {}
    return context_from(classify(prompt, port))


def route_subagent(packet):
    if os.environ.get("LAYA_CODEX_DISABLED") == "1":
        return {"status": "fallback", "reason": "disabled"}
    config, port = settings()
    if config.get("subagent_routing_enabled", False) is not True:
        return {"status": "fallback", "reason": "disabled"}
    task = packet.get("task") if isinstance(packet, dict) else None
    if not isinstance(task, str) or not task.strip() or len(task) > 4000:
        return {"status": "fallback", "reason": "invalid_packet"}
    result = classify(task, port, "subagent")
    if result is None:
        return {"status": "fallback", "reason": "unavailable"}
    return validate_selection(result)


def main(subagent=False):
    try:
        limit = MAX_TASK_BYTES if subagent else MAX_EVENT_BYTES
        raw = sys.stdin.buffer.read(limit + 1)
        if len(raw) > limit:
            raise ValueError("Input too large")
        packet = json.loads(raw.decode("utf-8-sig"))
        output = route_subagent(packet) if subagent else handle_event(packet)
    except (OSError, ValueError, TypeError, RecursionError, http.client.HTTPException):
        output = {"status": "fallback", "reason": "unavailable"} if subagent else {}
    if subagent:
        output.update(decision_id=uuid.uuid4().hex, executed=False)
    print(json.dumps(output, ensure_ascii=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(subagent="--route-subagent" in sys.argv[1:]))
