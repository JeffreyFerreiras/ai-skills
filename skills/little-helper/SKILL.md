---
name: little-helper
description: Delegate one tiny, precisely specified job to a fast verified free OpenCode model, with Muse Spark as the automatic availability fallback. Honor explicit model, provider, and effort choices. Use for narrow helper tasks, not general implementation, open-ended research, or independent review.
---

# Little Helper

Delegate exactly one narrow job that can safely be stopped when the parent has defined the objective and acceptance result. The parent owns planning, scoping, and acceptance. Editing or inspecting this skill does not itself authorize dispatch. Little Helper does not cover independent review and does not use Laya or its fallback.

## Runtime and model selection

Default to a separate **OpenCode CLI session**, not a native Codex collaboration agent. Resolve `opencode` from PATH and inspect `opencode --version`, `opencode models`, and the relevant `opencode models <provider> --verbose` metadata. If the CLI is absent, report the dependency; do not install it or silently switch runtimes.

1. Default to the exact listed model `opencode/deepseek-v4.1-flash-free` with a minimum effort of `high`; the calling agent may select `max` for the job. Favor correct tool use and low observed end-to-end latency for the bounded task; a single timing sample is not a universal model ranking. Verify that it is active, supports tools, and currently lists zero input, output, and applicable cache token costs. The paid `opencode/deepseek-v4.1-flash` and `opencode-go/deepseek-v4.1-flash` are different models.
2. If the default is unavailable before dispatch, another already verified suitable free model may be used when its correctness and latency justify the choice. Do not exhaustively search or benchmark the catalog on each job. `opencode/glm-5.3-flash-free` is an alternative, not a mandatory intermediate attempt. State the selected ID and reason. A name ending in `free` alone is insufficient evidence; missing costs are unknown, not zero. Verify current first-party pricing if metadata is incomplete.
3. When suitable verified free choices are unavailable, automatically use the listed `opencode-go/muse-spark-1.3-contributor` model with a minimum effort of `xhigh`. This Muse Spark fallback is authorized by the user's standing choice; no additional confirmation is needed. State the fallback and reason. Muse Spark is not free. If the exact fallback is unavailable, report the blocker rather than substituting an arbitrary paid or native model. Free availability and provider limits can change; do not promise unlimited use or lower total task cost.

Explicit user choices override defaults independently. For an explicitly requested Muse Spark worker, honor its exact listed model ID; otherwise use `opencode-go/muse-spark-1.3-contributor` when listed, following the installed `opencode-muse-spark` skill when available. An explicit exact-model-only instruction disables the default fallback; report its unavailability instead of substituting. Do not change global OpenCode models, provider configuration, or credentials.

For each job, permit at most one automatic fallback after dispatch, to Muse Spark, for a confirmed model/provider availability failure, such as a missing model, exhausted free quota, rate limit, or service outage. First verify that the original process is terminal and the scoped workspace is unchanged, with no partial or external effects. Start a fresh Muse session with the same bounded job and original absolute deadline. Do not reset the 120-second budget or retry the free model. If the first execution timed out, its state is unknown, it left effects, its answer failed verification, or no time remains, stop and report instead. Fallback is not a remedy for denied permissions or an invalid task.

The effort floors are **`high` for DeepSeek and other free helpers, and `xhigh` for Muse Spark**. The calling agent selects a supported effort at or above the relevant floor before each dispatch and records its choice and reason in the job packet. For DeepSeek, choose `high` or `max` according to the job's reasoning demands. Choose `max` when ambiguity, dependent reasoning, competing hypotheses, or hard-to-detect errors warrant more exploration; do not force it for every job. Muse currently exposes `xhigh` as its highest listed variant; do not invent a `max` setting for it. Pass the selected value through `--variant` after checking model support.

Effort labels are model-specific. Assess reasoning capability with representative tasks and independently checked answers; measure actual latency rather than assuming `max` or `xhigh` is slow. Task length alone does not determine the selected effort, and a simple job does not permit going below the floor. Neither a lookup nor a small successful pilot proves a general intelligence ranking or a quality gain from higher effort.

Honor an explicit supported user effort override independently of the model choice, including a user-requested exception to a floor. The calling agent cannot lower a floor on its own. Apply these floor checks only when the user has not explicitly requested a supported effort below the floor. A free model with no supported effort meeting its floor is unsuitable; follow the availability fallback. If Muse cannot meet its floor, report the blocker. Do not silently lower the selected effort, inherit a different default, invent a variant, or pass native `reasoning_effort` arguments to OpenCode. Narrow an overly broad job before selecting a larger model. Reject every GPT-5.6 variant, even if listed.

Use native Codex only when the user explicitly requests that runtime or a model uniquely identified as native. Use only callable supported model and effort values; for an ambiguous model offered by both runtimes, retain OpenCode. Native jobs have a minimum effort of `high`, with the calling agent selecting a higher supported effort when warranted unless the user overrides it. This is an opt-in route, not the default runtime or an automatic fallback.

## Job packet

Prepare a self-contained packet with:

1. A one-sentence objective and one to five ordered steps.
2. The workspace, owned paths, allowed actions and access, including read/write limits and exclusions.
3. The exact expected result or artifact and one focused acceptance check.
4. The selected runtime, exact provider/model ID, effort floor, chosen supported effort, and a short reason. For a free selection, record the current cost evidence.
5. The start time and absolute deadline, a stop condition, and a short report format: status, result, validation evidence, changed paths, and blockers.

If safe acceptance cannot be stated, narrow the packet before dispatch. Never ask the helper to choose the broader plan. Tell it that it is not alone in the workspace, must preserve other edits, and must stop on an unexpected condition. Prohibit subdelegation, scope expansion, background processes, extra research, and unrequested retries. The worker has no authority to commit, push, publish, or change unrelated files or settings.

## Dispatch and reuse

Use a fresh worker by default. Inspect repository status and relevant existing files before a writing job; for a non-repository workspace, record the scoped baseline needed to verify changes. Pass only the bounded job, not unrelated conversation history.

For OpenCode, use `opencode run` with the exact selected `--model`, the built-in `--agent build` unless the user specifies another agent, `--dir`, and `--format json` to retain events and session identity. Include `--auto` for this user's delegated workers unless they request manual approval. This approves permissions not explicitly denied; it does not expand the job's scope, override a denial, or bypass host permissions. Do not use `--share`.

Pass the packet as data, not interpolated shell code. In PowerShell a single-quoted here-string can hold the prompt. For DeepSeek, select `high` or `max` for the job and pass that exact value. This example selects the minimum:

```powershell
$helperPrompt = @'
<self-contained job packet, including the absolute deadline>
'@
$helperVariant = 'high'
opencode run --model opencode/deepseek-v4.1-flash-free --variant $helperVariant --agent build --auto --format json --dir 'C:\path\to\workspace' --title 'Little Helper: scoped job' $helperPrompt
```

For native Codex, use `collaboration.spawn_agent` with `fork_turns: "none"` and the exact selected supported model and reasoning effort. OpenCode model IDs cannot be passed to native spawn.

Reuse only for a new small job in the same task family when useful context remains, runtime/model/effort match, and allowed paths and actions remain within established boundaries. The new objective need not be identical. Require trusted prior dispatch, exact identity, and a verified terminal, idle state with no unfinished work. For OpenCode, continue only the recorded session using `--session <exact-id>`; never use unscoped `--continue`. For native Codex, use `collaboration.followup_task` with the exact identity. Supply a complete new packet each time. Recheck free status before another free dispatch. Start fresh when these conditions cannot be verified. Never reuse a running, unknown, interrupted, abandoned, or timed-out execution, or use reuse for independent review.

## Hard deadline and supervision

Each delegated job has a hard **120-second elapsed-time limit**, shared by any automatic fallback. Record the actual start time immediately before the first dispatch and include the computed absolute deadline in every execution packet. Dispatch and fallback overhead count. Parent preparation and final acceptance checks are outside this limit. Neither reuse nor fallback can reset an unfinished job's deadline.

Supervise the same foreground process or native agent. While it is pending, monitor only progress and the deadline. Use real clock readings, recalculate the remaining time before waits and after tool returns, and never request a wait beyond that remainder. Native `wait_agent` needs at least 10 seconds; use a short sleep for a smaller remainder or interrupt early. A prompt deadline alone does not enforce a process timeout: use host process controls to bound the OpenCode process and its children, retain its identity, and prevent detached work.

At the deadline, if no terminal result was received in time, mark the job `ABANDONED_TIMEOUT` and stop it. For native Codex, call `collaboration.interrupt_agent`. For OpenCode, use the host's cancellation or process-tree termination controls for the recorded worker and verify whether it stopped. Interruption is not rollback and may leave partial file changes or child processes. If termination is uncertain, report unknown state and do not start a duplicate or fallback. A late result cannot be accepted. Do not delegate work that cannot safely be abandoned.

Standard is the preferred speed, but inherit configured speed unless the selected host exposes a supported selector. Report `standard unverified/unavailable` when it does not. An explicit Fast request changes the preference only; apply it through a supported selector or report `fast unverified/unavailable` and inherited speed. Neither OpenCode's effort variant nor the native model choice proves speed. Do not invent speed flags or `service_tier` arguments.

## Accept and report

Accept only a terminal result received before the deadline that passes the parent's one focused verification. For OpenCode, retain the process exit status and session ID; exit code zero or a successful launch alone is insufficient. Check events for model, provider, permission, or execution failures and verify the actual expected result. For writing jobs, inspect the scoped diff and flag unexpected paths without reverting unrelated edits. For read-only jobs, verify the scoped baseline is unchanged. Do not run broad checks.

Report the exact runtime/model, effort, free-cost evidence when applicable, speed limitation, terminal/exit status, result, changed paths, and verification outcome concisely. If Muse fallback was used, include the availability failure that triggered it. Stop on unexpected conditions, mismatches, timeouts, or provider errors outside the defined availability fallback, and report the blocker and possible partial effects. Do not retry, switch models/providers, or expand the assignment beyond that one authorized fallback.
