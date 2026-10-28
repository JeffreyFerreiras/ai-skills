# Conditional Muse dispatch

Read this reference only when one or more reviewers may run as Muse Spark workers through the OpenCode CLI. Native-only panels skip it.

In this skill, a worker means a separate OpenCode CLI session coordinated and verified by the calling agent. It is not a nested collaboration agent inside an existing session unless the host provides one.

## Verify before dispatch

1. Resolve `opencode` from `PATH` and read `opencode --version`. If it is absent, stop and report the missing dependency. Do not install it without a request.
2. List available models with `opencode models`. Confirm the exact requested Muse Spark model is listed. Prefer `opencode-go/muse-spark-1.3-contributor` only when the user did not name a model and it is listed. Do not invent a model ID.
3. Confirm the requested effort variant is available for that model. Honor an explicit `xhigh` request when listed. `xhigh` is a supported explicit choice, not a silent default. Never substitute another model or effort silently.
4. Use the `build` agent unless the user names another OpenCode agent.

If any check fails, do not dispatch. Propose a supported replacement and keep the affected role uncovered rather than claiming silent coverage.

## Bound each worker

Translate the frozen packet into a concrete goal, allowed paths, exclusions, output contract, read-only condition, and stop condition. State that other agents may work in the checkout and that unrelated changes must stay intact. Give no commit, push, publish, broad-delete, or profile authority unless the user authorized it. Apply the user's approval preference (for example `--auto` only under their standing request). Approval preference never widens scope.

Give each worker an imperative prompt with compact labeled blocks. First-pass prompts carry the frozen target only, never another reviewer's report, finding, disposition, or verdict, even when the record already holds some. Follow-up prompts go out only after the initial phase is locked, only for a persisted follow-up request, and carry only that request's finding ids with their evidence, disposition, and reason, plus explicitly selected related findings. Use this shape:

```text
ROLE: You are an independent <role> reviewer. Make no edits and spawn no further agents.
GOAL: <role focus from reviewer-contracts.md plus frozen acceptance focus>
WORKSPACE: <absolute path>
SCOPE: <allowed paths; read-only investigation>
PRESERVE: Existing user changes and everything outside scope.
CONSTRAINTS: Read applicable AGENTS.md files. Do not commit, push, publish, or perform destructive cleanup.
INPUTS: <frozen base/head, hashes, evidence paths>
ACCEPTANCE: <testable role output contract>
VALIDATE: No commands; explain the evidence inspected.
STOP: Stop after the scoped report is complete or immediately on a blocker.
RETURN: Role, findings with file and line plus trigger/impact/remedy, coverage, open questions, and blockers. Be concise.
```

Run each worker from the intended workspace in a separate session. In PowerShell, keep the prompt in a here-string so task text passes as data:

```powershell
$musePrompt = @'
<bounded prompt>
'@
opencode run --model opencode-go/muse-spark-1.3-contributor --variant xhigh --agent build --auto --dir 'C:\path\to\workspace' --title 'Review panel: <role>' $musePrompt
```

Replace model, variant, agent, directory, title, and prompt with the verified values. Pass arguments directly to `opencode`. Prompt limits are cooperative, not tool confinement.

## Supervise workers

Announce each concrete task, role, workspace, selected model, and selected effort before dispatch. Keep selected values distinct from observed values. Claim observed values only from trusted runtime metadata; otherwise report them as unavailable.

Assess shared mutable resources before parallel starts. On a known terminal database-lock failure, stagger starts. Preserve each worker's reports and logs.

Wait for the same process to finish and retain its exit status and output. On timeout or interruption, treat the outcome as unknown. Inspect process, session, and workspace state before any retry. Never launch a duplicate while the first outcome is unknown. Reopening an unknown role for a late report needs that settlement first: confirm the old worker stopped, then record the reopen reason before redispatching. After return, inspect status and the scoped diff, flag out-of-scope changes, and treat worker claims as evidence to check. For read-only roles, the workspace must show no unexpected change.
