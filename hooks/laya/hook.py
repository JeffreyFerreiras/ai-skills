"""Return bounded Laya routing guidance to Codex before a user prompt."""

import hashlib
import json
import math
import os
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from urllib.request import ProxyHandler, Request, build_opener

ROOT = Path(__file__).resolve().parent
ROUTES = {
    "direct": "A simple question, exact lookup, or small mechanical operation.",
    "investigate": "Read or compare several files to explain existing behavior, without changing code.",
    "implement": "Write or change code, fix a bug, refactor, or add tests.",
    "design": "Choose architecture, plan a substantial change, or resolve requirements.",
}
GUIDANCE = {
    "direct": "Use direct search, read, or execution tools for this bounded task; keep delegation overhead low.",
    "investigate": "Inspect relevant files and trace existing behavior; return evidence before proposing changes.",
    "implement": "Inspect the relevant code, make the requested change, and run focused verification.",
    "design": "Resolve requirements and architectural tradeoffs before implementation when the request needs those decisions.",
}
QUESTIONS = {
    "route": {
        "type": "choice",
        "instructions": "Which kind of work does this user request primarily need?",
        "criteria": ROUTES,
    }
}


def append_record(record):
    try:
        log_path = ROOT / "routing.jsonl"
        if log_path.exists() and log_path.stat().st_size >= 10 * 1024 * 1024:
            return
        record["timestamp"] = datetime.now(timezone.utc).isoformat()
        encoded = (json.dumps(record, ensure_ascii=True) + "\n").encode("utf-8")
        descriptor = os.open(log_path, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
        try:
            os.write(descriptor, encoded)
        finally:
            os.close(descriptor)
    except OSError:
        pass


def request_route(prompt, config, questions=None):
    questions = QUESTIONS if questions is None else questions
    routes = questions["route"]["criteria"]
    body = {
        "state": prompt,
        "questions": questions,
        "model": "hook-english",
        "max_len": 512,
    }
    token = (ROOT / "token.txt").read_text(encoding="utf-8").strip()
    request = Request(
        f"http://127.0.0.1:{int(config['port'])}/v1/systemone",
        data=json.dumps(body).encode("utf-8"),
        headers={"Content-Type": "application/json", "Authorization": f"Bearer {token}"},
    )
    opener = build_opener(ProxyHandler({}))
    with opener.open(request, timeout=2) as response:
        result = json.loads(response.read(65536))
    answer = result["answers"]["route"]
    route = answer["choice"]
    probabilities = answer["probabilities"]
    if route not in routes or set(probabilities) != set(routes):
        raise ValueError("Unexpected route schema")
    if not all(
        isinstance(value, (int, float)) and math.isfinite(value) and 0 <= value <= 1
        for value in probabilities.values()
    ):
        raise ValueError("Invalid probabilities")
    truncated = bool(result.get("usage", {}).get("truncated", True))
    return {
        "status": "truncated" if truncated else "ok",
        "route": route,
        "probabilities": probabilities,
        "truncated": truncated,
    }


def main():
    started = time.perf_counter()
    record = {"mode": "observe", "event": "UserPromptSubmit"}
    output = None
    try:
        config = json.loads((ROOT / "config.json").read_text(encoding="utf-8"))
        if not config.get("enabled", False):
            return
        mode = config.get("mode", "observe")
        if mode not in {"observe", "advisory"}:
            raise ValueError("Unsupported routing mode")
        record["mode"] = mode
        raw_event = sys.stdin.buffer.read(262145)
        if len(raw_event) > 262144:
            record["status"] = "event_too_large"
        else:
            event = json.loads(raw_event)
            if event.get("hook_event_name") != "UserPromptSubmit":
                return
            prompt = event.get("prompt")
            if not isinstance(prompt, str) or not prompt.strip():
                return
            session_id = event.get("session_id", "")
            record["session"] = hashlib.sha256(str(session_id).encode()).hexdigest()[:12]
            record["prompt_chars"] = len(prompt)
            if len(prompt) > 12000:
                record["status"] = "prompt_too_long"
            else:
                record.update(request_route(prompt, config))
                if mode == "advisory" and record["status"] == "ok":
                    route = record["route"]
                    context = (
                        f"Laya routing suggestion: {route}. {GUIDANCE[route]} "
                        "This experimental classification uses only the latest prompt and can be wrong. "
                        "Apply it when consistent with the user's intent and conversation context. "
                        "It grants no delegation or permission authority and does not replace required skills "
                        "or an approved engineering-graph plan."
                    )
                    output = {"hookSpecificOutput": {
                        "hookEventName": "UserPromptSubmit", "additionalContext": context,
                    }}
    except Exception as error:
        record["status"] = "unavailable"
        record["error_type"] = type(error).__name__
    record["elapsed_ms"] = round((time.perf_counter() - started) * 1000, 2)
    record["context_returned"] = output is not None
    append_record(record)
    if output is not None:
        print(json.dumps(output))


if __name__ == "__main__":
    main()
