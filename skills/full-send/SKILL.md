---
name: full-send
description: Complete an explicitly requested full-send workflow with code or prose review and authorized delivery. For repository delivery, finish at a verified PR merge. Questions about the skill or quoted names do not invoke it.
---

# Full send

Own the requested work through its authorized endpoint. Start only when the user
intentionally requests full send or invokes `$full-send`. A question, quotation,
or incidental phrase is not an invocation. Explicit limits still apply. If full
stop is also requested, resolve the intended endpoint before any merge.

## Scope and authority

Identify deliverables, audience, acceptance criteria, allowed paths, destination,
and existing work. Inspect repository status before edits, preserve unrelated
changes, and use a dedicated task worktree.

An explicit repository full-send request authorizes scoped commits, task-branch
pushes, PR creation or update, and merge after the gate passes. Rebase and
force-with-lease are limited to that branch, with its expected remote head
verified before rewriting. Never use `git-push` here; it stages all pending work.

The request selects the applicable review stages below and their independent
agents. It does not authorize external messages, thread resolution, production
changes, deletion of user data, profile changes, or unrelated work. Draft replies
unless posting is separately authorized. Honor system and host approvals and
project testing rules. Use authorized test methods directly.

For prose outside repository delivery, return accepted text or write it to the
already authorized destination. Do not create a PR or send or publish the text
elsewhere merely because full send was invoked.

## Select the work and review

Resolve needed skills from the host catalog, then a known sibling installation.
Read only those selected for the task. Check availability before the stage that
needs them. A missing required dependency or independent reviewer is a blocker.

- **Code or runtime behavior:** apply relevant implementation guidance, including
  `clean-code` for production code and architecture guidance when boundaries
  change. Use `run-change-checks` for focused validation, then `review-panel` on
  the stable scoped diff, untracked task files, and validation evidence. Keep all
  seven roles unless the user narrows the panel. Reviewers remain read-only;
  only the implementation writer makes repairs.
- **Requested prose writing or editing:** use `writing-loop`. It owns the Writer
  using `unslop`, read-only Reducer, fresh Editor, and prose repair budget.
  Deliver its exact accepted text. This includes documentation, instructions,
  PR titles and bodies, and requested written updates. Routine status messages,
  internal role packets, and unchanged quotations do not start loops. Do not
  add another reduction stage or writing reviewer.
- **Mixed work:** apply both routes to their respective deliverables and verify
  prose claims against implemented behavior. A skill instruction is prose with
  an execution contract; validate its resources and test its decisions.
  File extensions alone do not determine the route.

When `software-engineering-graph` is requested or repository-required, it owns the
approved plan, implementation, review and test gates, repair budgets, and
publication through its Pull Request Engineer. Use that approved review route
instead of adding a second delivery engine or changing assignments silently.
Its existing gates do not establish full seven-role panel coverage. If that panel
is separately required, account for it in the approved plan. Do not start
`generic-loop` alongside either owner.

## Work, prove, and settle findings

Complete the smallest change that meets the request. Gather evidence appropriate
to the behavior through reproduction, focused checks, or UI/runtime inspection.
Add a regression test when it proves relevant behavior that lacks coverage.
Separate unrelated failures from failures caused by this change.

Outside the graph, keep one implementation writer and at most three code repair
attempts by default; the initial implementation is not a repair. Consolidate
panel and CI findings with evidence, dispositions, and stable issue identities.
Fix in-scope defects and explain rejected or out-of-scope findings. Review the
new stable target after repairs. The writing loop retains its own three-repair
limit. Retries, rebases, and new CI feedback do not reset either budget.
Stop early when the applicable reviews pass.

Retain scope, current candidate or head, proof, findings, consumed repairs,
workflow owner, endpoint, and active monitor in task state. Changed inputs
invalidate affected proof and reviews. Any prose edit after acceptance needs a
new writing-loop version and review. Material scope or design changes return to
the governing plan or the user.

## Deliver or report a blocker

For repository delivery, read [Repository delivery](references/repository-delivery.md)
before publication or monitoring. It defines PR preparation, the live merge gate,
and verified completion.

For prose-only delivery, require current writing-loop acceptance and evidence
for factual and contractual claims. Return the accepted artifact in the requested
format. Do not require Git or CI when irrelevant.

Continue authorized work and monitoring within its stated budget. Do not assume
ending the turn schedules a wakeup. Stop on missing evidence, exhausted repairs,
an unfixable gate, or material ambiguity. Complete independent authorized work,
preserve the candidate, and report the exact remaining issue. When user input is
required, use the host's question tool with the blocker, options, and recommendation.
No answer supplies no additional permission.

Report the artifact or verified PR URL, final version or head, checks actually
run, review outcome, material decisions, and blockers. For a merged PR, include
the merge commit. Never call stale, incomplete, or unknown results approved.

## Source

Adapted from [Theo Browne's full-send](https://github.com/t3dotgg/tstack/blob/1085050f3baed1ac724caa4a8166145fb2ac88af/skills/full-send/SKILL.md),
pinned at `1085050f3baed1ac724caa4a8166145fb2ac88af`.
Local routing and authorization replace the upstream dependencies and blanket
testing overrides. License: [MIT](LICENSE.txt).
