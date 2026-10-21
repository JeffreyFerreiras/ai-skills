"""Resident offline CUDA classifier, with a native Windows Service entry point."""
from __future__ import annotations

import argparse
import contextlib
import json
import os
from pathlib import Path
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, HTTPServer

ROOT = Path(__file__).resolve().parent
MAX_BODY_BYTES = 8192
MAX_PROMPT_CHARS = 1200
MODELS = {"routine": "gpt-6-luna", "standard": "gpt-6.1-sol", "demanding": "gpt-6-astra"}
QUESTIONS = {
    "task_type": {"type": "choice", "instructions": "What is the main deliverable requested in request?",
                  "criteria": {"editing": "rename, format, or mechanically transform existing content",
                               "implementation": "implement features or tests with specified behavior",
                               "debugging": "investigate a failure, find its cause, and repair it",
                               "architecture": "design systems, assess technical tradeoffs or invariants",
                               "explanation": "explain, summarize, or document existing information",
                               "writing": "compose new nontechnical prose or messages"}},
    "difficulty": {"type": "choice", "instructions": "How much reasoning does request require? Judge ambiguity and interacting constraints, not prompt length or tool use.",
                   "criteria": {"routine": "clear mechanical change, basic fact, short text, obvious solution",
                                "standard": "bounded implementation or analysis, several steps, familiar solution",
                                "demanding": "subtle root cause, competing constraints, deep reasoning, uncertain solution"}},
}


def recommend(agent, prompt):
    started = time.perf_counter()
    prediction = agent.predict({"request": prompt}, QUESTIONS)
    if prediction.get("usage", {}).get("truncated") is not False:
        return {"status": "abstain_truncated"}
    answers = prediction["answers"]
    task = answers["task_type"]["choice"]
    difficulty = answers["difficulty"]["choice"]
    if task not in QUESTIONS["task_type"]["criteria"] or difficulty not in MODELS:
        raise ValueError("Invalid model response")
    model = MODELS[difficulty]
    if difficulty == "routine" and task in ("debugging", "architecture"):
        model = MODELS["standard"]
    return {"status": "experimental_suggestion", "model": model, "task_type": task,
            "difficulty": difficulty, "inference_ms": round((time.perf_counter() - started) * 1000, 2)}


class ClassifierServer(HTTPServer):
    # One request at a time bounds memory and serializes all GPU access.
    request_queue_size = 4
    allow_reuse_address = True

    def __init__(self, port, classifier):
        super().__init__(("127.0.0.1", port), Handler)
        self.classifier = classifier
        self.requests_completed = 0
        self.timeout = 0.25


class Handler(BaseHTTPRequestHandler):
    def setup(self):
        super().setup()
        self.connection.settimeout(1.0)

    def log_message(self, *_args):
        pass  # Never log prompts, request paths, or response bodies.

    def reply(self, status, value):
        body = json.dumps(value, ensure_ascii=True).encode()
        try:
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        except OSError:
            pass  # The hook can time out and disconnect during inference.

    def do_GET(self):
        if self.path != "/health":
            return self.reply(404, {})
        self.reply(200, {"ready": True, "device": "cuda", "pid": os.getpid(),
                         "startup_ms": getattr(self.server, "startup_ms", None),
                         "requests_completed": self.server.requests_completed})

    def do_POST(self):
        if self.path != "/classify":
            return self.reply(404, {})
        try:
            length = int(self.headers.get("Content-Length", "0"))
            if self.headers.get("Transfer-Encoding") or not 0 < length <= MAX_BODY_BYTES:
                return self.reply(413, {})
            raw = self.rfile.read(length)
            if len(raw) != length:
                return self.reply(400, {})
            body = json.loads(raw)
            prompt = body.get("prompt") if isinstance(body, dict) else None
            if not isinstance(prompt, str) or not prompt.strip() or len(prompt) > MAX_PROMPT_CHARS:
                return self.reply(400, {})
        except (ValueError, OSError, RecursionError):
            return self.reply(400, {})
        try:
            result = self.server.classifier(prompt)
            self.server.requests_completed += 1
            self.reply(200, result)
        except Exception:
            self.reply(503, {})


def serve(root, port, stopped, ready=lambda: None):
    started = time.perf_counter()
    # SCM must launch the real interpreter, not the venv redirector's child.
    # Use that environment's packages without depending on shell activation.
    packages = root / "runtime" / "Lib" / "site-packages"
    if packages.is_dir():
        sys.path.insert(0, str(packages))
    os.environ.update(HF_HOME=str(root / "service-data" / "cache"), HF_HUB_OFFLINE="1",
                      TRANSFORMERS_OFFLINE="1", USE_TF="0", PYTHONDONTWRITEBYTECODE="1")
    import laya
    import torch

    if not torch.cuda.is_available():
        raise RuntimeError("CUDA is unavailable")
    agent = laya.load(str(root / "weights"), device="cuda")
    recommend(agent, "Rename a variable.")
    if stopped.is_set():
        return
    # Bind only after loading and warmup. A successful health response means ready.
    with ClassifierServer(port, lambda prompt: recommend(agent, prompt)) as server:
        server.startup_ms = round((time.perf_counter() - started) * 1000, 2)
        ready()
        while not stopped.is_set():
            server.handle_request()


def windows_service(root, port):
    import ctypes
    from ctypes import wintypes

    advapi = ctypes.WinDLL("advapi32", use_last_error=True)
    callback = ctypes.WINFUNCTYPE(None, wintypes.DWORD, ctypes.POINTER(wintypes.LPWSTR))
    handler_type = ctypes.WINFUNCTYPE(wintypes.DWORD, wintypes.DWORD, wintypes.DWORD,
                                     wintypes.LPVOID, wintypes.LPVOID)

    class Status(ctypes.Structure):
        _fields_ = [(name, wintypes.DWORD) for name in (
            "type", "state", "controls", "exit_code", "specific_exit", "checkpoint", "wait_hint")]

    class Entry(ctypes.Structure):
        _fields_ = [("name", wintypes.LPWSTR), ("main", callback)]

    advapi.RegisterServiceCtrlHandlerExW.argtypes = [wintypes.LPCWSTR, handler_type, wintypes.LPVOID]
    advapi.RegisterServiceCtrlHandlerExW.restype = wintypes.HANDLE
    advapi.SetServiceStatus.argtypes = [wintypes.HANDLE, ctypes.POINTER(Status)]
    advapi.SetServiceStatus.restype = wintypes.BOOL
    advapi.StartServiceCtrlDispatcherW.argtypes = [ctypes.POINTER(Entry)]
    advapi.StartServiceCtrlDispatcherW.restype = wintypes.BOOL
    stopped = threading.Event()
    handle = None

    def report(state, code=0):
        status = Status(0x10, state, 5 if state == 4 else 0, code, 1 if code == 1066 else 0,
                        1 if state in (2, 3) else 0, 120000 if state in (2, 3) else 0)
        advapi.SetServiceStatus(handle, ctypes.byref(status))

    @handler_type
    def control(code, _event, _data, _context):
        if code in (1, 5):
            report(3)
            stopped.set()
        return 0

    @callback
    def main(_argc, _argv):
        nonlocal handle
        handle = advapi.RegisterServiceCtrlHandlerExW("LayaCodex", control, None)
        if not handle:
            return
        report(2)
        try:
            with open(os.devnull, "w") as sink, contextlib.redirect_stdout(sink), contextlib.redirect_stderr(sink):
                serve(root, port, stopped, lambda: report(4))
        except Exception as error:
            # Store only the exception class and OS code, never model input or paths.
            try:
                (root / "service-data" / "startup-error.txt").write_text(
                    f"{type(error).__name__}; winerror={getattr(error, 'winerror', None)}\n", encoding="utf-8")
            except OSError:
                pass
            report(1, 1066)
            return
        report(1)

    table = (Entry * 2)(Entry("LayaCodex", main), Entry(None, callback()))
    if not advapi.StartServiceCtrlDispatcherW(table):
        raise ctypes.WinError(ctypes.get_last_error())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--port", type=int, default=8766)
    parser.add_argument("--windows-service", action="store_true")
    args = parser.parse_args()
    if not 1 <= args.port <= 65535:
        parser.error("port must be between 1 and 65535")
    if args.windows_service:
        windows_service(args.root.resolve(), args.port)
    else:
        serve(args.root.resolve(), args.port, threading.Event())


if __name__ == "__main__":
    main()
