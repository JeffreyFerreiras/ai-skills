---
name: review-panel
description: Run a seven-role independent read-only review panel with merged evidence. Use when the user explicitly requests a panel, multi-role, or parallel review. Do not use for solo reviews, implementation, or repairs.
---

# Review Panel

Run a requested seven-role read-only panel and merge its evidence. This skill never authorizes edits, fixes, commits, publication, or profile changes.

Use only when the user explicitly asks for a panel, multi-role review, parallel reviewers, or names this skill. A solo review, implementation, or repair request never activates it. Auditing this skill does not start a panel run.

The full panel is seven independent reviewers plus one merging Supervisor:

- code quality, architecture, naming, bug hunting, logic, counterexamples, verification.
- The Supervisor verifies and merges evidence. It does not vote, repair, or decide by counts.

Run all seven unless the user explicitly narrows the scope. Do not merge roles silently. A role with nothing relevant reports no findings plus its coverage limit. Silence never counts as approval.

- For role contracts and finding bars, read [reviewer-contracts.md](references/reviewer-contracts.md).
- When any reviewer may run outside the current session, read [opencode-dispatch.md](references/opencode-dispatch.md) before dispatch.

## 1. Freeze the review target

Record the exact repository path, base revision, head revision or PR head, allowed paths, and working-tree status. Default to read-only with no permitted edits. Preserve unrelated work. Use an isolated checkout when review reads would disturb consumer work.

Hash mutable inputs (diff, logs, generated files) and bind each hash to the review identity. Recompute before accepting results. A stale PR head or changed source invalidates affected results. Report the invalidation and re-review the new target.

Completion: every reviewer receives the same frozen identity, scope, hashes, and evidence paths.

## 2. Dispatch fresh reviewers

Start each reviewer in a fresh context with the user intent and raw evidence only. Do not share author reasoning, a preferred verdict, a prior conclusion, or another reviewer's output. Instruct each reviewer to inspect surrounding code as needed, make no edits, and spawn no further agents unless the user authorized delegation.

Prompt limits on paths and effects are cooperative instructions. Do not call them sandbox enforcement.

Completion: each reviewer starts from the frozen packet without cross-reviewer leakage.

## 3. Select and announce runtime

Support user-selected native, Muse, or mixed assignments. Verify each exact model and effort against a trusted listing before dispatch. Honor an explicit Muse `xhigh` request. Muse `xhigh` is one supported choice, not a default for unrelated work. Never silently substitute or install a model, effort, or tool. Do not carry one run's model outcome into new selections as a universal rule.

Immediately before each dispatch, state the concrete task, reviewer role, workspace, exact selected model, and exact selected effort. Keep selected values distinct from observed values. Report observed values only from trusted runtime metadata; otherwise report them as unavailable.

Completion: every dispatch has an announced role, workspace, selected model, and selected effort, all verified as available.

## 4. Supervise the run

Give each reviewer a bounded prompt with goal, allowed paths, exclusions, acceptance focus, output contract, and stop condition. Use separate sessions for parallel reviewers. Apply the user's approval preference without widening scope. Assess shared mutable resources before parallel starts. On a known terminal database-lock failure, stagger starts. Preserve reports and logs.

Wait for the same worker to finish and retain its exit status and output. On timeout or interruption, treat the outcome as unknown. Inspect worker and workspace state before any retry. Never launch a duplicate while the first outcome is unknown.

A review-only run authorizes no extra approval gate. Edits, fixes, publication, or profile changes require separate user intent. Only an explicitly authorized verifier may run focused commands or disposable reproductions in disposable paths. Keep panel reports outside the installed skill and unrelated source.

When the request also authorizes fixes or publication, finish the read-only panel first. Keep panel workers read-only, then continue through the appropriate separate workflow using the authorization already given.

Completion: every reviewer has a terminal known outcome, or is reported as unknown with its evidence gap.

## 5. Merge and report

Group concordant findings by verified cause, not by vote count. Label each candidate as accepted, rejected, unresolved, or out of scope. Preserve dissent, incomplete gates, exact checks run, exact code state, and limitations. State what each role actually inspected. End with a clear recommendation and residual risks.

An unavailable or silent review never counts as approval. A review is not a repair loop or a delivery run.

Completion: the report accounts for all seven roles, each candidate disposition, dissent, coverage limits, and the frozen review identity.
