"""Fast, fail-open Codex hook client for the resident loopback Laya service."""
from __future__ import annotations

import http.client
import json
import os
import sys

TIMEOUT_SECONDS = 0.75
MAX_PROMPT_CHARS = 1200
MAX_EVENT_BYTES = 1024 * 1024
MAX_RESPONSE_BYTES = 8192
MODELS = {"routine": "gpt-6-luna", "standard": "gpt-6.1-sol", "demanding": "gpt-6-astra"}
TASKS = {"editing", "implementation", "debugging", "architecture", "explanation", "writing"}


def classify(prompt, port):
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=TIMEOUT_SECONDS)
    try:
        connection.request("POST", "/classify", body=json.dumps({"prompt": prompt}).encode(),
                           headers={"Content-Type": "application/json"})
        response = connection.getresponse()
        if response.status != 200:
            return None
        raw = response.read(MAX_RESPONSE_BYTES + 1)
        return json.loads(raw) if len(raw) <= MAX_RESPONSE_BYTES else None
    finally:
        connection.close()


def context_from(result):
    if not isinstance(result, dict) or result.get("status") != "experimental_suggestion":
        return {}
    task, difficulty, model = (result.get(name) for name in ("task_type", "difficulty", "model"))
    if not all(isinstance(value, str) for value in (task, difficulty, model)):
        return {}
    if task not in TASKS or difficulty not in MODELS:
        return {}
    expected = MODELS["standard"] if difficulty == "routine" and task in ("debugging", "architecture") else MODELS[difficulty]
    if model != expected:
        return {}
    message = f"Laya recommends {model} ({task}, {difficulty}). The active Codex model has not changed."
    context = (
        message + " This is experimental English-only advice, with no calibrated accuracy claim. "
        "For a substantive new task, delegate the work to one subagent "
        f"using model {model} and fork_turns=none. Give the worker the full task, relevant "
        "conversation context, workspace path, constraints, and required checks. Tell it not "
        "to delegate further. Wait for its result and report the outcome in this chat. If you "
        "are already a delegated worker, do the work yourself and do not route again. Handle "
        "status questions, short clarifications, cancellation, and instructions to change "
        "routing directly. Do not create a separate desktop chat. Honor explicit user model "
        "choices, instructions against delegation, review-only limits, authorization, and "
        "higher-priority rules. The classifier sees only this prompt, not conversation history. "
        "The primary model owns verification. If model selection or subagents are unavailable, "
        "explain the limitation and continue with the current model. Never claim the active "
        "chat model changed."
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
    port = int(os.environ.get("LAYA_CODEX_PORT", "8766"))
    return context_from(classify(prompt, port)) if 1 <= port <= 65535 else {}


def main():
    try:
        raw = sys.stdin.buffer.read(MAX_EVENT_BYTES + 1)
        output = {} if len(raw) > MAX_EVENT_BYTES else handle_event(json.loads(raw))
    except (OSError, ValueError, TypeError, RecursionError, http.client.HTTPException):
        output = {}
    print(json.dumps(output, ensure_ascii=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
