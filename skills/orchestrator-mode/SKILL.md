---
name: orchestrator-mode
description: Orchestrate with the primary only thinking and dispatching little helpers via the OpenCode CLI using a selectable model. Use when the user asks for orchestrated delegation or requires the primary to delegate all reads, writes, and checks. Do not use for direct implementation or single-agent work.
---

# Orchestrator Mode

The primary thinks and orchestrates. Little helpers use the OpenCode CLI to handle the task's file reads, research, edits, and validation.

Invoke as `$orchestrator-mode <task>` for automatic free-first routing, or `$orchestrator-mode model=<provider/model> <task>` to pin the model for every helper and follow-up. The choice applies to helpers only. Verify the full ID in the runtime catalog. If the pinned model is unavailable or ambiguous, report the blocker and ask for a replacement. Resolve conflicts with fixed-role plans before dispatch.

## Primary boundary

The primary may plan, compose job packets, dispatch and monitor workers, accept evidence, and communicate. For dispatch preflight, the primary may load `$little-helper` and inspect CLI help, model metadata, and provider terms. Delegate task instructions, file and Git inspection, task research, edits, and validation to helpers. Keep implementation authority separate from design-only work.

## Job packets

Give each little helper job a self-contained packet: a one-sentence objective, 1 to 5 exact ordered steps, owned paths and allowed actions, the exact expected output with acceptance criteria, one focused verification, an absolute 120-second deadline, and stop conditions. Do not claim native spawn supports unavailable model identifiers. A longer workflow is multiple bounded jobs; do not call one long-running worker a 120-second little job.

## Dispatch

Use `$little-helper` for runtime discovery, free-model eligibility, effort, and availability fallback. With automatic routing, start with a verified free model when one fits and is available. Share confirmed quota or availability failures across jobs in the same orchestration so helpers can use the remaining free models before the authorized paid fallback. An exact orchestrator pin overrides Little Helper's model defaults and exclusions and disables automatic substitution. Verify the pinned model's availability and data terms. Honor a supported user-selected effort. For a pinned model with no Little Helper effort floor, prefer `xhigh` when supported unless the user selects another effort. Use `model=opencode/muse-ember-1.0` to pin paid Muse Ember, or use it after free choices when the user explicitly selects it as the paid fallback.

Run the worker from the intended workspace using Little Helper's CLI syntax. Prefer an isolated Git worktree when authorized or repository-required. Before every dispatch announce the worker name, bounded scope, exact model, and actual effort and speed availability.

## Supervise

For each running little job, monitor only that job and its deadline. On timeout interrupt the exact process, mark the job abandoned and unknown, wait for confirmed terminal state, then inspect partial effects through a separate bounded helper; never duplicate an unknown process. No detached background work.

## Authority and gates

Preserve authorization, unrelated edits, and existing scope. No installs or syncs, paid activation, commit, push, PR, deploy or production changes, or external messages unless the corresponding user intent exists. If a required graph or generic-loop skill applies, have a helper read and report its actual preflight and approval rules and missing capabilities; never bypass gates, simulate approvals, or change the framework. For new or updated skills require the generic-loop with one immutable Writer, a fresh non-author Reviewer each pass, frozen focused validations, max 3 repairs, and an exact APPROVE, REVISE, or BLOCK decision. Capture artifacts in the repo-conventional location with handoff evidence; do not install or sync the skill automatically. Report unavailable model or effort enforcement as a limitation; ask for the required choice when a workflow actually blocks instead of silently bypassing.
