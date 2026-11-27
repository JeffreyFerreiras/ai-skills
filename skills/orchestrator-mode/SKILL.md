---
name: orchestrator-mode
description: Orchestrate with the primary only thinking and dispatching little helpers via the OpenCode CLI using a selectable model. Use when the user asks for orchestrated delegation or requires the primary to delegate all reads, writes, and checks. Do not use for direct implementation or single-agent work.
---

# Orchestrator Mode

The primary thinks and orchestrates. Little helpers use the OpenCode CLI to handle the task's file reads, research, edits, and validation.

Invoke as `$orchestrator-mode <task>` to match each helper to the job's difficulty and prefer suitable free routes, or `$orchestrator-mode model=<provider/model> <task>` to pin the model for every helper and follow-up. The choice applies to helpers only. Verify the full ID in the runtime catalog. If the pinned model is unavailable or ambiguous, report the blocker and ask for a replacement. Resolve conflicts with fixed-role plans before dispatch.

## Primary boundary

The primary may plan, compose job packets, dispatch and monitor workers, accept evidence, and communicate. For dispatch preflight, the primary may load `$little-helper` and inspect CLI help, model metadata, and provider terms. Delegate task instructions, file and Git inspection, task research, edits, and validation to helpers. Keep implementation authority separate from design-only work.

## Job packets

Give each little helper job a self-contained packet with a one-sentence objective, 1 to 5 exact ordered steps, owned paths and allowed actions, the expected output with acceptance criteria, one focused verification, and stop conditions. Record the start time immediately before first dispatch and give the job an absolute deadline 150 seconds later, including dispatch and retry overhead. This Orchestrator Mode limit overrides Little Helper's standalone 120-second limit. Split longer workflows into bounded jobs. For automatic routing, record the difficulty, capability floor, evidence, chosen route and effort, and why they fit.

## Dispatch

Use `$little-helper` for runtime, provider and data checks, effort floors, dispatch, and safe fallback. The policy below replaces its default model order only for orchestration. Apply these choices only to roles without a fixed assignment. Honor a supported user-selected effort independently of the model choice and assess automatic candidates at that effort.

1. Set the capability floor for each bounded job before considering price. Use the [benchmark catalog](references/model-catalog.md) to classify the job as basic, involved, or difficult. Consider ambiguity, dependent reasoning, and how hard an error would be to detect. Specify the required tools, context, modalities, domain evidence, and fit within the deadline. Narrow oversized jobs first.
2. Verify that each candidate's exact route and supported effort meet the floor and Little Helper's requirements. Use the catalog and relevant benchmark results for the kind of work. For specialized work, set the required domain evidence before ranking routes. Use a versioned task benchmark or independently checked results from comparable jobs when the general score is missing or insufficient. An unverified alias or effort is a research lead. Refresh the relevant entries when drift or missing evidence could change the decision. For the same route, prefer the lowest supported effort that meets the floor and job requirements unless the job's reasoning demands or evidence about the deadline favor another eligible effort. Give task-specific evidence priority over differences in composite scores.
3. Prefer a currently available route with verified zero input, output, and applicable cache charges among the candidates that meet the floor. Prefer stronger job evidence and deadline fit within that set. If no suitable free route remains, use a suitable authorized paid route and state why. The standing paid fallback is `opencode-go/muse-spark-1.3-contributor` at `xhigh` or higher when the evidence and data terms permit it. Other paid routes require user authorization for their use, even if already enabled. If no eligible authorized route meets the floor, report the gap and pause that job. Keep the capability floor independent of price and availability.
4. After a confirmed free-route availability failure, share the known failure with other jobs in the orchestration. Allow at most one automatic retry, and only after confirming the failed process exited without partial workspace or external effects. Apply the same floor and free-first order to the remaining candidates. Start a fresh OpenCode session for the same job, update the packet's routing details, and keep the original absolute deadline. Apply Little Helper's fallback stop conditions.

An exact orchestrator pin overrides automatic model defaults and exclusions and disables substitution. Verify the pinned model's availability and data terms. For a pinned model with no Little Helper effort floor, prefer `xhigh` when supported unless the user selects another effort. A user-selected paid fallback such as `opencode/muse-ember-1.0` remains subject to the capability floor during automatic selection.

Run the worker from the intended workspace using Little Helper's CLI syntax. Prefer an isolated Git worktree when authorized or repository-required. Before every dispatch announce the worker name, bounded scope, exact model, and actual effort and speed availability.

## Supervise

For each running little job, monitor only that job and its deadline. On timeout interrupt the exact process, mark the job abandoned and unknown, wait for confirmed terminal state, then inspect partial effects through a separate bounded helper; never duplicate an unknown process. No detached background work.

## Authority and gates

Preserve authorization, unrelated edits, and existing scope. No installs or syncs, paid activation, commit, push, PR, deploy or production changes, or external messages unless the corresponding user intent exists. If a required graph or generic-loop skill applies, have a helper read and report its actual preflight and approval rules and missing capabilities; never bypass gates, simulate approvals, or change the framework. For new or updated skills require the generic-loop with one immutable Writer, a fresh non-author Reviewer each pass, frozen focused validations, max 3 repairs, and an exact APPROVE, REVISE, or BLOCK decision. Capture artifacts in the repo-conventional location with handoff evidence; do not install or sync the skill automatically. Report unavailable model or effort enforcement as a limitation; ask for the required choice when a workflow actually blocks instead of silently bypassing.
