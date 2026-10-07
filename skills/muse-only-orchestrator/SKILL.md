---
name: muse-only-orchestrator
description: Orchestrate with the primary only thinking and dispatching little Muse helpers via the OpenCode CLI. Use when the user asks for Muse-only orchestration or requires the primary to delegate all reads, writes, and checks. Do not use for direct implementation or single-agent work.
---

# Muse Only Orchestrator

The primary thinks and orchestrates only. Every file read, research step, write or edit, and validation runs as a little Muse helper through the OpenCode CLI. The primary never touches files or runs checks itself.

## Primary boundary

The primary may plan, reason, compose job packets, dispatch and monitor workers, accept evidence, and communicate. The primary must not directly read files, research, write or edit files, or perform validation. Delegate even instruction and skill reads and Git inspection to helpers. Runtime discovery may be done by a helper; the primary may launch and monitor processes as orchestration without filesystem inspection. Keep implementation authority separate from design-only work.

## Job packets

Give each little Muse job a self-contained packet: a one-sentence objective, 1 to 5 exact ordered steps, owned paths and allowed actions, the exact expected output with acceptance criteria, one focused verification, an absolute 120-second deadline, and stop conditions. Never pretend native collaboration.spawn_agent supports Muse. A longer workflow is multiple bounded jobs; do not call one long-running worker a 120-second little job.

## Dispatch

Invoke the OpenCode CLI with a real listed Muse model, preferred opencode-go/muse-spark-1.3-contributor, the build agent, and automatic approval only within authority unless manual approval was requested. Default effort xhigh for Muse helpers via the verified --variant xhigh selector; resolve real model and variant support before dispatch and report or ask when unsupported, never silently substitute. Speed has no verified selector; report speed as unavailable or unverified. Prefer an isolated Git worktree when authorized or repository-required; do not create one from mere read-only review without authorization. Before every dispatch announce the concrete worker name, bounded scope, exact model, and actual effort and speed availability.

## Supervise

For each running little job, monitor only that job and its deadline. On timeout interrupt the exact process, mark the job abandoned and unknown, wait for confirmed terminal state, then inspect partial effects through a separate bounded helper; never duplicate an unknown process. No detached background work.

## Authority and gates

Preserve authorization, unrelated edits, and existing scope. No installs or syncs, paid activation, commit, push, PR, deploy or production changes, or external messages unless the corresponding user intent exists. If a required graph or generic-loop skill applies, have a helper read and report its actual preflight and approval rules and missing capabilities; never bypass gates, simulate approvals, or change the framework. For new or updated skills require the generic loop with one immutable Writer, a fresh non-author Reviewer each pass, frozen focused validations, max 3 repairs, and an exact APPROVE, REVISE, or BLOCK decision. Capture artifacts in the repo-conventional location with handoff evidence; do not install or sync the skill automatically. Report unavailable model or effort enforcement as a limitation; ask for the required choice when a workflow actually blocks instead of silently bypassing.
