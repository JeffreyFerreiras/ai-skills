---
name: to-tickets
description: Turn a plan, spec, or conversation into verifiable implementation tickets with blockers. Use for work breakdown and ticket drafts; publish only when explicitly requested.
---

# To Tickets

Produce tickets that deliver narrow, complete paths through the affected system. Each ticket must name its real blockers and describe an outcome that can be demonstrated or verified.

## Gather the source

Use the supplied plan, spec, conversation, or issue. Read referenced source material and relevant comments when accessible. If a source is unavailable, state the gap rather than inventing requirements. Inspect affected code only as needed to understand current behavior, dependencies, and useful prefactoring. Use established glossary terms and relevant ADRs.

Honor an existing tracker, templates, labels, and local draft location. No setup skill or particular tracker is required. If none is specified, prepare a local draft; ask for a destination only when publishing needs it.

## Slice the work

- Each ordinary ticket covers a narrow end-to-end behavior across the layers that behavior needs, including verification. A backend-only product need not invent a UI.
- A completed ticket is demonstrable or verifiable on its own, and small enough for one focused implementation session.
- Avoid horizontal tickets such as "all schemas" followed by "all APIs" when a working vertical slice can land.
- Include prefactoring only when it enables the work. Give it a concrete preservation check and block only tickets that actually need it.
- Give each ticket explicit ticket dependencies and unresolved decision preconditions. A ticket can start only when its dependencies are complete and required decisions are settled. No ticket dependencies alone does not establish readiness. Distinguish required dependencies from a merely preferred work order, and check the graph for cycles.

### Wide mechanical refactors

When one change affects many callers and no vertical slice can land passing checks, use **expand, migrate, contract**:

1. Expand: add the new form alongside the old, preserving compatibility.
2. Migrate: move callers in bounded batches, such as a package or directory. Each batch depends on expand; keep checks passing because the old form still exists.
3. Contract: remove the old form only after every migration batch is done and no caller remains. It depends on all batches.

If batches cannot pass independently, explain why. Keep the sequence on a shared integration branch and make every batch block a final integrate-and-verify ticket. State that passing checks are promised at that integration point, not each batch. Drafting this plan does not authorize branch creation or implementation.

## Make the draft reviewable

Number tickets in dependency order with stable local IDs. For each, include:

- Title and the end-to-end outcome.
- Observable acceptance criteria and the check or demo that establishes them.
- Blocker IDs and titles, or "None".
- Material assumptions, unresolved requirements, or risks that affect implementation. Mark unresolved decisions needed to start as implementation preconditions; do not invent answers or label the ticket ready until they are settled.

Use [the ticket format](references/ticket-format.md) when the project has no template. Avoid brittle code recipes and exact paths unless they identify a necessary contract. A small prototype snippet may be included when it records a decision more precisely than prose; label its source and omit demo scaffolding.

Return the concrete draft before asking about material uncertainty. Incorporate feedback on granularity, blockers, merges, and splits. Do not require a generic approval interview when the user already specified the breakdown. A draft can be ready for review while individual tickets remain blocked or require a decision.

For local files, use the project's established draft location. Without one, use `.scratch/<feature-slug>/issues/<NN>-<slug>.md`, one file per ticket, if local file output is requested or appropriate to the task. Otherwise return the draft in chat. Check existing files and use an unused destination or preserve current drafts.

## Publish only within explicit authorization

A request to break work into tickets or approval of a breakdown alone does not authorize external publication. When the user explicitly asks to create issues, use the specified tracker and the reviewed draft, in dependency order so blockers can reference real IDs.

Use native blocking links where supported, otherwise include textual references. Use existing labels only when appropriate; do not create a label or impose `ready-for-agent`. Link sub-issues to a source parent only when authorized by the request and supported by tracker conventions. Do not otherwise edit or close the parent.

Before retrying an uncertain issue creation, check whether it already exists to avoid duplicates. If publishing stops partway, report created IDs and remaining drafts, preserving their blocker mapping. Ticket planning does not authorize working the tickets, modifying code, or installing other skills.

## Source

Adapted from Matt Pocock's [to-tickets skill](https://github.com/mattpocock/skills/tree/6fd947921b935b7e1e69293a200400f0fdd5c15f/skills/engineering/to-tickets), pinned at `6fd947921b935b7e1e69293a200400f0fdd5c15f`. Upstream file: `SKILL.md`. Distributed under the [MIT license](LICENSE.txt).
