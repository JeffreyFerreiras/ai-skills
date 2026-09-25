---
name: little-helper
description: Delegate one tiny, precisely specified execution job to a fresh subagent with optional model and effort overrides. Use when the user asks for a narrow helper task; do not use for general implementation, open-ended research, or independent review.
---

# Little Helper

Delegate exactly one narrow, stop-safe execution job when the parent has already defined the objective and acceptance result. The parent remains responsible for planning, scoping, and accepting the result.

Default selection: model=`gpt-6-sol`, effort=`low`, speed preference=`fast`. Model and effort may each be overridden independently when the requested value is supported by the callable spawn tool.

## Job packet

Before dispatch, make the packet self-contained and include:

1. A one-sentence objective.
2. One to five exact, ordered steps.
3. Allowed paths and actions, including read/write limits.
4. The exact expected result or artifact.
5. One focused verification that establishes that result.
6. The resolved model and effort selection after applying any supplied overrides. Use only values supported by the callable spawn tool; do not silently substitute a model or effort.

If any item is missing, the work is open-ended, or safe acceptance cannot be stated, narrow the packet before dispatch. Never ask the helper to decide the broader plan.

## Dispatch and deadline

Use `collaboration.spawn_agent` with `fork_turns: "none"`, a fresh self-contained prompt, and the exact supported model and reasoning-effort arguments. Do not include unrelated context. Tell the worker it is not alone in the codebase, identify its owned paths, and require it to preserve other edits. Prohibit subdelegation, scope expansion, background work, extra research, and unrequested retries.

Every job that uses a tool—including file access, commands, or web search—is an execution job. Record the current time immediately before dispatch and enforce a hard 120-second elapsed-time limit from that point. Include the computed absolute deadline in the worker prompt so it can keep each command within the remaining budget. The spawn tool has no deadline argument, so while the child is pending, monitor only its progress and deadline; do not start other work. Use real clock readings, not your reasoning-time estimates, and keep every wait within the remaining time. `wait_agent` requires at least 10 seconds; use a short sleep for a final remainder when available or interrupt early. Never wait beyond the deadline.

At 120 seconds, if no terminal result has been received before the deadline, call `collaboration.interrupt_agent`, mark the job `ABANDONED_TIMEOUT`, and report possible partial effects. A completion observed after the deadline is late and cannot be accepted. Interruption is not rollback and does not guarantee that an external process was killed. Do not delegate work that cannot safely be abandoned. Constrain commands to the remaining time where supported; prohibit detached or background processes.

There is no speed selector in the native spawn tool. Treat `fast` as a preference only: inherit the configured speed, and report `fast unverified/unavailable` when the tool does not expose a speed setting. Do not invent a `service_tier` or other unsupported argument, and do not claim that selecting Luna enables Fast mode.

## Accept and report

Accept a result only if the terminal result was received before the deadline and the exact expected result passes the single focused verification. Keep verification focused after the terminal result; do not run broad checks. Otherwise report the mismatch or blocker without expanding the assignment. Require a short worker report with status, result or artifact, verification evidence, changed paths, and any blocker.

If the worker finds an unexpected condition, it must stop and return a concise blocker. Do not authorize extra steps or retries implicitly.
