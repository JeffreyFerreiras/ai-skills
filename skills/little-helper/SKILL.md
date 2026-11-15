---
name: little-helper
description: Delegate one tiny, precisely specified job through OpenCode. Prefer verified free models before the authorized paid fallback. Honor explicit model, provider, and effort choices, including Muse workers. Use for narrow helper tasks, not general implementation, open-ended research, or independent review.
---

# Little Helper

Delegate exactly one narrow job that can safely be stopped when the parent has defined the objective and acceptance result. The parent owns planning, scoping, and acceptance. Editing or inspecting this skill does not itself authorize dispatch. Little Helper does not cover independent review and does not use Laya or its fallback.

## Runtime and model selection

Default to a separate **OpenCode CLI session**. Resolve `opencode` from PATH and check `opencode --version`, `opencode models`, and the model metadata for the intended workspace. OpenCode v2 exposes the detailed catalog through `opencode api model.list`. Verify the full ID, enabled and active status, tool support, supported effort, context and output limits, and applicable input, output, and cache prices. Check that the selected provider's regions and data terms fit the complete prompt and data the worker may access. Consult the appropriate source for [Console](https://opencode.ai/docs/console/models/#privacy), [Go](https://opencode.ai/docs/console/go/#privacy), or another selected provider. If the CLI is absent, report the dependency.

1. Prefer `opencode/deepseek-v4.1-flash-free` when suitable. A free route needs verified zero input, output, and applicable cache token prices. If metadata is incomplete, verify pricing and terms against [OpenCode Console free models](https://opencode.ai/docs/console/models/#free-models).
2. If DeepSeek is unsuitable or unavailable, choose another verified free model that fits the task. Consider `opencode/glm-5.3-flash-free` for bounded coding and `opencode/minimax-m3.1-flash-free` when the job needs a larger context window. `opencode/inkling-2-small-free` fits bounded jobs where its regional restrictions and data terms allow it. Its provider retains requests and may use them for training. Allow other verified suitable free models as the catalog changes. Reuse recent evidence within the orchestration and check fresh evidence when terms or availability change.
3. Use the authorized paid fallback when no suitable verified free choice remains. The standing fallback is `opencode-go/muse-spark-1.3-contributor` at `xhigh` or higher, subject to the job's data requirements. Use a different paid fallback only if the user selects it, such as `opencode/muse-ember-1.0` on Console. State why the paid route was selected. If the authorized fallback is unavailable, report the blocker.

Explicit user choices override defaults independently. For an explicit Muse Spark request, use the exact model ID if the user supplied one. Otherwise prefer `opencode-go/muse-spark-1.3-contributor`, then another listed Muse Spark model that meets the effort requirement. State the selected ID. If the exact requested model is unavailable or no suitable Muse Spark model is listed, report the blocker.

An explicit exact-model-only instruction disables the default fallback; report unavailability instead of substituting. Do not change global OpenCode models, provider configuration, or credentials.

After a confirmed availability failure on a free route, such as exhausted quota, rate limiting, or service outage, allow at most one automatic fallback for that job. First confirm the process exited and left no partial workspace or external effects. Reapply the same order with the remaining suitable free models first. Start a fresh session with the same job and absolute deadline. Carry quota and availability failures forward within the orchestration until a known reset or confirmed availability. A catalog listing alone does not establish that quota reset. Treat a failure as model-specific unless the provider reports a wider scope. Stop on a timeout, unknown state, effects, failed acceptance check, denied permissions, invalid task, or elapsed deadline.

The effort floors are **`high` for DeepSeek and other free helpers** and **`xhigh` for Muse Spark and Muse Ember**. Select a supported effort at or above the floor and record the reason in the job packet. For DeepSeek, choose `high` or `max` according to the job's reasoning demands. Choose `max` when ambiguity, dependent reasoning, competing hypotheses, or hard-to-detect errors warrant more exploration. Verify the model's available variants and the CLI's effort syntax before dispatch.

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

For OpenCode, run `opencode run` from the intended workspace with the exact selected `--model`, `--agent build` unless the user specifies another agent, and `--format json` to retain events and session identity. Use the command runner's working directory or set the shell location before starting. Include `--auto` unless the user requests manual approval. Apply it within the job's existing authority. Keep the session private.

Pass the packet as data. In PowerShell a single-quoted here-string can hold the prompt. OpenCode v2 accepts `provider/model#variant` as one `--model` argument. If an older CLI exposes `--variant` in its run help, pass the ID and effort separately. This v2 example selects `high` for DeepSeek:

```powershell
$helperPrompt = @'
<self-contained job packet, including the absolute deadline>
'@
$helperModel = 'opencode/deepseek-v4.1-flash-free#high'
opencode run --model $helperModel --agent build --auto --format json --title 'Little Helper: scoped job' $helperPrompt
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

Report the exact runtime/model, effort, free-cost evidence when applicable, speed limitation, terminal/exit status, result, changed paths, and verification outcome concisely. If a fallback was used, include the availability failure that triggered it. Stop on unexpected conditions, mismatches, timeouts, or provider errors outside the defined availability fallback, and report the blocker and possible partial effects.
