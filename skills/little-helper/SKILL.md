---
name: little-helper
description: Delegate one tiny, precisely specified execution job to a fresh or selectively reused subagent with optional model and effort overrides. Use when the user asks for a narrow helper task; do not use for general implementation, open-ended research, or independent review.
---

# Little Helper

Delegate exactly one narrow, stop-safe execution job when the parent has already defined the objective and acceptance result. The parent remains responsible for planning, scoping, and accepting the result. Use a fresh helper by default. Reuse a settled helper only for a new small related job when the objective, allowed files, actions and access, selected model and effort all match, and its context remains useful. Require trusted prior dispatch and identity plus a terminal idle status. Otherwise start fresh. Never reuse a running, unknown, interrupted, abandoned, or timed-out execution. Start fresh for unrelated work, changed boundaries or model/effort, stale or cluttered context, and any independent review under its separate reviewer workflow. Little Helper does not cover independent review.

Select the lowest sufficient supported model and effort for the bounded job. Use `gpt-6-luna` at `low` for deterministic file or skill lookups, inventories, and exact mechanical actions. Increase Luna's effort only when the job needs interpretation or comparison. Use the current workhorse `gpt-6.1-sol` at `low` or `medium` only when a concrete correctness or implementation need exceeds Luna's suitability, and state why. Avoid `xhigh` or `max` for straightforward lookups. Do not choose a larger model to solve an overly broad job; narrow the job first. Do not claim lower total cost or add unsupported price information. Model and effort may each be overridden independently by explicit user choices when supported by the callable spawn tool. Reject every GPT-5.6 variant, even when the runtime still exposes it; request a supported current selection instead.

## Job packet

Before dispatch, make the packet self-contained and include:

1. A one-sentence objective.
2. One to five exact, ordered steps.
3. Allowed paths and actions, including read/write limits.
4. The exact expected result or artifact.
5. One focused verification that establishes that result.
6. The selected model and effort, plus one short reason tied to the job's actual complexity. Apply any explicit overrides independently. Use only values supported by the callable spawn tool; do not silently substitute a model or effort.

If any item is missing, the work is open-ended, or safe acceptance cannot be stated, narrow the packet before dispatch. Never ask the helper to decide the broader plan.

## Dispatch and deadline

For a fresh helper, use `collaboration.spawn_agent` with `fork_turns: "none"`, a fresh self-contained prompt, and the exact selected supported model and reasoning-effort arguments. Do not include unrelated context. Tell the worker it is not alone in the codebase, identify its owned paths, and require it to preserve other edits. Prohibit subdelegation, scope expansion, background work, extra research, and unrequested retries. For an eligible settled helper, use `collaboration.followup_task` with its exact existing identity and a complete new job packet; preserve its ownership and require it to preserve other edits. Do not inherit or rely on incomplete prior task instructions.

Every distinct job that uses a tool—including file access, commands, or web search—is an execution job. Immediately before each fresh dispatch or reuse, record the current time and set that job's hard 120-second elapsed-time limit. Include the computed absolute deadline in the packet. Reuse never resets or extends the deadline of an unfinished job; unfinished executions are ineligible for reuse. While that job is pending, monitor only its progress and deadline; do not start other work. Use real clock readings, not reasoning-time estimates, and keep every wait within the remaining time. `wait_agent` requires at least 10 seconds; use a short sleep for a final remainder when available or interrupt early. Never wait beyond the deadline.

At 120 seconds, if no terminal result has been received before the deadline, call `collaboration.interrupt_agent`, mark the job `ABANDONED_TIMEOUT`, and report possible partial effects. A completion observed after the deadline is late and cannot be accepted. Interruption is not rollback and does not guarantee that an external process was killed. Do not delegate work that cannot safely be abandoned. Constrain commands to the remaining time where supported; prohibit detached or background processes.

There is no speed selector in the native spawn tool. Request Standard as a preference only: inherit the configured speed, and report `standard unverified/unavailable` when the tool does not expose a speed setting. Do not invent a `service_tier` or other unsupported argument, and never claim that the selected model verifies native speed. Fast is not the default or preferred setting.
If the user explicitly requests Fast during planning, record that choice in the job packet and apply
it only through a supported host setting. Without a native selector, report `fast unverified/unavailable`
and inherited speed; do not silently switch settings or claim the preference was applied.

## Accept and report

Accept a result only if the terminal result was received before the deadline and the exact expected result passes the single focused verification. Keep verification focused after the terminal result; do not run broad checks. Otherwise report the mismatch or blocker without expanding the assignment. Require a short worker report with status, result or artifact, verification evidence, changed paths, and any blocker.

If the worker finds an unexpected condition, it must stop and return a concise blocker. Do not authorize extra steps or retries implicitly.
