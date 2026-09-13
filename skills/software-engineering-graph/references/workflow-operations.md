# Graph workflow operations

Read only the section needed for the current operation. The shared approval, dispatch-transparency,
role-skill, independence, and publication rules remain in `SKILL.md`.

## Task brief and design loop

The Supervisor records the objective, user outcome, included and excluded scope, constraints,
acceptance criteria, risk, authority, evidence paths, inspection budget, required checks,
specialists, and human decisions. Inspect the selected worktree and protect unrelated changes.

Each initial design generation, design revision, and delivery redesign begins with the fixed
architecture and validation research pair. The Supervisor assesses dependencies/resources, seals
their evidence collection, and then creates the same-generation Tech Lead branch. Advisory and
initial fast-path routes omit this research pair.

The Tech Lead produces current behavior, interfaces, data/control flow, failure handling,
compatibility, observability, rollout, rollback, alternatives, and test strategy. It writes only
the approved design artifact. The initial inspection ceiling is 12 file reads and 8 focused
discovery commands unless the task brief approves less; only a Supervisor follow-up may expand it.

The Architect and required read-only specialists independently return `APPROVE`, `REVISE`, or
`BLOCK`, with stable finding IDs and evidence. A blocking finding must identify the acceptance
criterion, in-scope surface, and concrete impact. The Supervisor returns malformed results for
correction without ingesting them, rejects scope expansion, deduplicates valid findings, and sends
one bounded revision packet. The Tech Lead may return `SCOPE_OBJECTION` with the controlling scope
text, missing causal link, and smallest in-scope alternative. The Supervisor adjudicates it.

The design loop allows at most three revision rounds. A material scope expansion requires the
human-authorized new brief and plan path. Implementation waits for design approval.

## Implementation and delivery review

The Senior Engineer receives the approved brief, design, acceptance criteria, and assigned findings.
It is the sole source/test writer, preserves unrelated changes, applies the mandatory role-skill
preflight from `SKILL.md`, runs focused checks, and reports design deviations before implementing
them. It does not commit, push, open a PR, or remove a worktree.

At a stable checkpoint, dispatch a fresh Code Reviewer and Test Engineer in parallel only when their
resources do not conflict. Add required read-only specialists. The Code Reviewer checks correctness,
regressions, design fidelity, maintainability, security implications, and test adequacy. The Test
Engineer independently maps acceptance criteria to evidence and distinguishes regressions from
unrelated failures. Reviewers and testers do not repair their own findings.

The Supervisor deduplicates findings and sends one coherent repair packet to the Senior Engineer.
Return each repair to the independent gate that raised it. Repeat only affected checks unless a new
change, failure, or unresolved risk justifies broader verification. The delivery loop allows at most
three repair rounds. Material interface, dependency, persistence, security, deployment, or scope
changes return to design.

## Concurrency and evidence

Before each graph fan-out, assess data dependencies plus writable paths, mutable state, build output,
databases, ports, devices, and constrained external services. Serialize conflicts or record an
explicit dependency. Keep one writer. Give each node bounded inputs, actions, outputs, inspection
budgets, and a stopping condition.

Start independent reviewers and specialists with fresh context reconstructed from verified task,
design, stable diff/checkpoint, acceptance, and evidence artifacts. Do not pass worker reasoning or
Supervisor narration. Same-role repairs may retain that role's own context. Research nodes return
evidence manifests only and never write, test, decide, create findings, or spawn children.

Prefer repository evidence with stable file, line, command, log, or artifact references. Keep noisy
exploration in role sessions and return compact packets with exact failures, contradictions,
provenance, uncertainty, and truncation. External, destructive, production, costly, and scope-
expanding effects remain explicit authority boundaries.

## Closure and handoffs

Close only when every acceptance criterion has evidence, the approved design still matches, no
blocking or major finding remains, required checks pass, unrelated failures are separated, and
rollout/rollback/approval conditions are satisfied. Repository delivery also requires exactly one
review-ready PR created or the exact existing PR updated and verified through the publication
contract. A publication blocker means incomplete delivery.

Minimum handoffs:

- task brief: scope, non-goals, constraints, acceptance, risk, authority, evidence and budgets;
- research: verified evidence manifest, null decision, and empty findings;
- design/review: proposal or decision, stable findings, evidence, revisions and unresolved choices;
- implementation: changed files, acceptance mapping, checks, deviations, risks and skill usage;
- code review/test: independent decision, evidence, gaps, conformance and required skill usage;
- helper: the selected canonical helper packet and registered lifecycle, never a parent decision;
- closure: delivered outcome, validation, residual risk, approvals and next action.

For reviewer-initiated conditional fan-out, the primary reviewer freezes preliminary findings and
submits the exhaustive ID-only request. The Supervisor validates, dispatches, seals, and consolidates
the collection. Never pass raw authority, paths, prompts, operation IDs, or dispatch control data.
