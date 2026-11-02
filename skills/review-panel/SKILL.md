---
name: review-panel
description: Run a seven-role independent read-only review panel with merged evidence. Use for an explicit panel request or the code-review stage selected by an invoked full-send/full-stop workflow. Do not use for ordinary solo reviews or as an implementation workflow.
---

# Review Panel

Run a requested seven-role read-only panel and merge its evidence. This skill never authorizes edits, fixes, commits, publication, or profile changes.

Start when the user explicitly asks for a panel, multi-role review, parallel reviewers, or names this skill. An intentionally invoked full-send or full-stop workflow may also select this panel as its code-review stage. That selection authorizes the review agents only; the delivery owner retains implementation, repairs, and publication authority. Ordinary solo review, implementation, or repair requests do not activate a panel. Auditing this skill does not start one.

The full panel is seven independent reviewers plus one merging Supervisor:

- code quality, architecture, naming, bug hunting, logic, counterexamples, validation audit.
- The Supervisor verifies and merges evidence. It does not vote, repair, or decide by counts.

Run all seven unless the user explicitly narrows the scope. Do not merge roles silently. A role with nothing relevant reports no findings plus its coverage limit. Silence never counts as approval.

- For role contracts and finding bars, read [reviewer-contracts.md](references/reviewer-contracts.md).
- When any reviewer may run outside the current session, read [opencode-dispatch.md](references/opencode-dispatch.md) before dispatch.
- For the per-run findings record, read [persistence.md](references/persistence.md). The Supervisor owns and writes that record with `scripts/review_state.py`.

## 1. Freeze the review target

Record the exact repository path, base revision, head revision or PR head, allowed paths, and working-tree status. Default to read-only with no permitted edits. Preserve unrelated work. Use an isolated checkout when review reads would disturb consumer work.

Hash mutable inputs (diff, logs, generated files) and bind each hash to the review identity. Recompute before accepting results. A stale PR head or changed source invalidates affected results. Report the invalidation and re-review the new target.

Completion: every reviewer receives the same frozen identity, scope, hashes, and evidence paths.

## 2. Dispatch fresh reviewers

Start each reviewer in a fresh context with the user intent and raw evidence only. Do not share author reasoning, a preferred verdict, a prior conclusion, or another reviewer's output. Build each first-pass packet with only the frozen identity and scope, never another reviewer's report or finding, even when a record already holds some. Instruct each reviewer to inspect surrounding code as needed, make no edits, and spawn no further agents unless the user authorized delegation.

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

The Supervisor is the only writer of the per-run record. Store each initial report once and never rewrite it. An unknown role never flips silently to covered; before the lock the host may reopen it with a reason only after settling the old execution. Lock the initial phase only when every role has reported or is explicitly marked unknown with a reason. No dispositions, replies, fix verification, or follow-up requests exist before the lock. After the lock, record each follow-up request before emitting its packet: send a reviewer the request's finding ids with their evidence, disposition, and reason, plus explicitly selected related findings. Record each reply with its stance; a dispute reopens its finding to unresolved. Silence or an unknown outcome never counts as agreement. A requested follow-up stays open until a linked reply arrives or the Supervisor records it unresolved with a reason. Pending requests block verified fixes and closing; close only with a fresh source and no pending requests.

Completion: every reviewer has a terminal known outcome, or is reported as unknown with its evidence gap.

## 5. Merge and report

Group concordant findings by verified cause, not by vote count. Label each candidate as accepted, rejected, unresolved, or out of scope. Preserve dissent, incomplete gates, exact checks run, exact code state, and limitations. State what each role actually inspected. Recompute the declared mutable-input hashes and compare the current source, including scope, with the frozen identity before accepting or reusing results. A changed revision, input, or scope marks every finding stale, resets fixed or verified states to open, and refuses further live decisions on the old target; preserve the original evidence, stable ids, and history, start a new verified run, and re-review the new target. End with a clear recommendation and residual risks.

An unavailable or silent review never counts as approval. A review is not a repair loop or a delivery run.

Completion: the report accounts for all seven roles, each candidate disposition, dissent, coverage limits, and the frozen review identity.
