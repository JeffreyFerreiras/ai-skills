---
name: orchestrator-mode
description: Orchestrate with the primary only thinking and dispatching little helpers via the OpenCode CLI using a selectable model. Use when the user asks for orchestrated delegation or requires the primary to delegate all reads, writes, and checks. Do not use for direct implementation or single-agent work.
---

# Orchestrator Mode

The primary thinks and orchestrates. Every file read, research step, write or edit, and validation runs as a little helper through the OpenCode CLI. The primary never touches files or runs checks itself.

Invoke as `$orchestrator-mode model=<provider/model> <task>`. `model=` is optional; when omitted use `opencode-go/muse-spark-1.3-contributor`. The choice selects every helper, never the primary chat; reuse it for every job and follow-up until the user changes it. Honor any exact listed model including valid non-Muse models. Verify the full `provider/model` against `opencode models`; report an unavailable or ambiguous model and ask, never silently substitute. An explicit choice takes precedence over automatic routing; resolve any fixed-role plan conflict without bypass.

## Primary boundary

The primary may plan, reason, compose job packets, dispatch and monitor workers, accept evidence, and communicate. The primary must not directly read files, research, write or edit files, or perform validation. Delegate even instruction and skill reads and Git inspection to helpers. Runtime discovery may be done by a helper; the primary may launch and monitor processes as orchestration without filesystem inspection. Keep implementation authority separate from design-only work.

## Job packets

Give each little helper job a self-contained packet: a one-sentence objective, 1 to 5 exact ordered steps, owned paths and allowed actions, the exact expected output with acceptance criteria, one focused verification, an absolute 120-second deadline, and stop conditions. Do not claim native spawn supports unavailable model identifiers. A longer workflow is multiple bounded jobs; do not call one long-running worker a 120-second little job.

## Dispatch

Invoke the OpenCode CLI with the verified selected model, the build agent, and automatic approval only within authority unless manual approval was requested. Verify the full provider/model against the runtime catalog before dispatch; resolve actual --variant support for that model and prefer xhigh when supported. Report effort or speed as unavailable or unverified when unsupported; never invent a DeepSeek effort value or silently alter an explicit choice. Only ask for a missing choice when the workflow truly blocks. Prefer an isolated Git worktree when authorized or repository-required; do not create one from mere read-only review without authorization. Before every dispatch announce the concrete worker name, bounded scope, exact model, and actual effort and speed availability.

Run it from the intended workspace. In PowerShell, keep the prompt in a here-string so shell metacharacters in the task are passed as data, and pass the selected model as one safely quoted data argument:

```powershell
$helperPrompt = @'
<bounded prompt>
'@
opencode run --model "opencode-go/muse-spark-1.3-contributor" --agent build --auto --dir 'C:\path\to\workspace' --title 'helper: scoped task' $helperPrompt
```

Replace the model with the verified selection, and the directory, title, and prompt with the resolved values. Pass arguments directly to `opencode`; do not build a second shell command from untrusted task or model text.

## Supervise

For each running little job, monitor only that job and its deadline. On timeout interrupt the exact process, mark the job abandoned and unknown, wait for confirmed terminal state, then inspect partial effects through a separate bounded helper; never duplicate an unknown process. No detached background work.

## Authority and gates

Preserve authorization, unrelated edits, and existing scope. No installs or syncs, paid activation, commit, push, PR, deploy or production changes, or external messages unless the corresponding user intent exists. If a required graph or generic-loop skill applies, have a helper read and report its actual preflight and approval rules and missing capabilities; never bypass gates, simulate approvals, or change the framework. For new or updated skills require the generic-loop with one immutable Writer, a fresh non-author Reviewer each pass, frozen focused validations, max 3 repairs, and an exact APPROVE, REVISE, or BLOCK decision. Capture artifacts in the repo-conventional location with handoff evidence; do not install or sync the skill automatically. Report unavailable model or effort enforcement as a limitation; ask for the required choice when a workflow actually blocks instead of silently bypassing.
