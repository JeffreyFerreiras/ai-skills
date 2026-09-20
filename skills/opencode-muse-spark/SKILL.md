---
name: opencode-muse-spark
description: Delegate a bounded task to a Muse Spark worker through the OpenCode CLI. Use when the user explicitly asks Codex to run, invoke, or delegate to an OpenCode Muse Spark subagent. Do not use for ordinary OpenCode help or generic multi-agent work.
---

# OpenCode Muse Spark

Run Muse Spark as an external delegated worker in the user's workspace. In this
skill, "subagent" means a separate OpenCode CLI session coordinated and verified
by the calling agent; it does not mean a Codex collaboration agent or an OpenCode
child session inside an existing OpenCode TUI.

## Select the runtime

1. Resolve `opencode` from `PATH` and read `opencode --version`. If it is absent,
   stop and report that dependency instead of installing it without a request.
2. List available models with `opencode models`.
3. Honor an exact Muse Spark model requested by the user. Otherwise prefer
   `opencode-go/muse-spark-1.3-contributor` when it is listed. If it is absent,
   choose another listed Muse Spark model, state the exact model selected, and do
   not invent a model ID.
4. Use the built-in `build` agent unless the user names another OpenCode agent.
   The separate CLI process is the delegated worker even though `build` is a
   primary agent within that OpenCode session.

## Bound the delegation

Before launching OpenCode:

- Translate the user's request into a concrete goal, allowed paths, exclusions,
  acceptance criteria, validation commands, and a stop condition. Carry through
  whether the task is read-only or permits edits.
- Inspect the current repository status and preserve pre-existing changes. Tell
  the worker that it is not alone in the workspace and must not revert unrelated
  edits.
- Do not give the worker permission to commit, push, publish, delete broadly, or
  perform another external mutation unless the user authorized that action.
- Do not add `--auto` by default. It auto-approves permissions that OpenCode has
  not explicitly denied. Use it only when the user explicitly asks for automatic
  approval and the requested scope supports that risk.

Give Muse an imperative prompt with this information in compact labeled blocks:

```text
ROLE: You are a delegated Muse Spark worker. Work only on the task below.
GOAL: <observable outcome>
WORKSPACE: <absolute path>
SCOPE: <allowed files or read-only investigation>
PRESERVE: Existing user changes and everything outside scope.
CONSTRAINTS: Read applicable AGENTS.md files. Do not commit, push, publish, or
perform destructive cleanup. Do not spawn further agents unless requested.
ACCEPTANCE: <testable criteria>
VALIDATE: <commands, or "no commands; explain the evidence inspected">
STOP: Stop after the scoped result is complete or immediately on a blocker.
RETURN: Outcome, files changed, validation results, and blockers. Be concise.
```

Run it from the intended workspace. In PowerShell, keep the prompt in a here-string
so shell metacharacters in the task are passed as data:

```powershell
$musePrompt = @'
<bounded prompt>
'@
opencode run --model opencode-go/muse-spark-1.3-contributor --agent build --dir 'C:\path\to\workspace' --title 'Muse Spark: scoped task' $musePrompt
```

Replace the model, agent, directory, title, and prompt with the resolved values.
Pass arguments directly to `opencode`; do not build a second shell command from
untrusted task text.

## Supervise and verify

Wait for the same process to finish and retain its exit status and output. If it
times out or is interrupted, treat the result as unknown, inspect the workspace
and OpenCode session state, and do not launch a duplicate worker until the first
run is known to be terminal.

After it returns:

1. Inspect repository status and the scoped diff. Flag any path outside the
   declared scope; do not silently accept or revert it.
2. Treat the worker's claims as evidence to check, not approval. Run the narrowest
   relevant validation independently when the task permits it.
3. For read-only work, verify that the workspace did not change.
4. Report the exact model, OpenCode exit status, outcome, changed paths,
   validation, and any residual risk or blocker.
