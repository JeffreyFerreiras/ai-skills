# Full-send and full-stop integration

The local delivery modes share one workflow. `full-send` completes authorized
delivery; `full-stop` prepares the reviewed result for the user's final action.
For repository delivery, that difference is verified merge versus an open,
unmerged PR. Prose-only work does not require a PR.

## Routing and ownership

| Work | Validation and review |
| --- | --- |
| Code or runtime behavior | Relevant implementation skills, `run-change-checks`, then `review-panel`. |
| Requested prose writing or editing | `writing-loop`, including Unslop, its Reducer, and a fresh Editor. Check factual and contractual claims too. |
| Mixed work | Both routes for their respective deliverables, with prose claims checked against the code. |

The seven-role [review-panel](../skills/review-panel/SKILL.md) can be selected by
an intentionally invoked delivery mode. It remains read-only. Ordinary code
implementation does not start a panel. The delivery owner consolidates findings
and returns repairs to one implementation writer.

[Writing-loop](../skills/writing-loop/SKILL.md) already provides
[Unslop](../skills/unslop/SKILL.md), reduction, and independent acceptance.
There is no separate writing-reducer skill. Requested prose cleanup, documentation,
instructions, PR titles and bodies, and written updates use that loop.
Routine status messages, internal packets, and unchanged quotations do not.

A skill instruction is prose with an execution contract. Validate resources and
test decisions as well as editing its wording. Route by behavior and purpose,
not file extension.

When [software-engineering-graph](../skills/software-engineering-graph/SKILL.md)
is requested or required, its approved plan owns implementation, reviews, tests,
repair budgets, and publication. Do not add another engine or claim its gates
cover all seven panel roles. Include any separately required panel in that plan.
Outside the graph, the delivery owner allows at most three code repairs by default;
the writing loop owns its own three-repair budget. Retries and CI feedback do not
reset them. Changed inputs invalidate affected evidence.

## Local dependencies

The upstream `sol-loop` stage is replaced by the selected local review route.
[Create-pull-request](../skills/create-pull-request/SKILL.md) replaces `file-pr`
and can create or update the exact task PR under delegated publication authority.
It commits selected changes; `git-push` remains for explicit all-pending-changes
requests.

[Babysit-pr](../skills/babysit-pr/SKILL.md) monitors checks and reviews with one
owned watcher and a default 30-minute window. It reports events to the delivery
owner. [Address-pr-feedback](../skills/address-pr-feedback/SKILL.md) supplies
thread-aware inspection and selected repairs. Neither monitoring nor delivery
implies permission to post comments or resolve threads.

Dependencies are resolved from the host catalog, then a known sibling installation.
Missing required skills or independent reviewers block the dependent stage.
No T3-specific tool or unavailable model is assumed.

## Authority and endpoints

Only an intentional workflow invocation starts delivery. Questions about the
skills, quotations, and incidental "full stop" emphasis do not. Explicit task
limits remain binding.

An invoked repository mode authorizes its scoped commits, task-branch pushes,
PR preparation, and lease-protected task-branch rewrites. Only full send grants
merge authority, and only after the live head/base gate passes. System and host
approvals and project testing rules remain in force.

Full stop disables existing auto-merge and removes the PR from its merge queue.
It verifies both states before pushes and handoff and never merges or pushes to
the default branch. It reports
ready, waiting for required approval, or blocked. Full send verifies actual merge
and its commit before completion, then follows active cleanup instructions.
Both preserve unrelated work and newer branch commits.

For prose, full send returns the accepted artifact or writes to an already
authorized destination. Full stop leaves it ready for the user's action.
Neither authorizes sending or external publication. Profiles are not changed by
adapting the repository skills.

## Provenance and verification

[Full-send](../skills/full-send/SKILL.md) and
[full-stop](../skills/full-stop/SKILL.md) are adapted from
[t3dotgg/tstack](https://github.com/t3dotgg/tstack/tree/1085050f3baed1ac724caa4a8166145fb2ac88af),
pinned at `1085050f3baed1ac724caa4a8166145fb2ac88af`, retrieved November 1, 2026.
Both retain Theo Browne's MIT license. Their local entrypoints replace upstream
dependencies, incidental triggers, and blanket testing overrides.

Repository metadata, links, and scenario checks validate packaging and selected
decisions. They cannot establish live GitHub monitoring, merge behavior, or
cross-host agent availability. Report those limits with the tested state.
