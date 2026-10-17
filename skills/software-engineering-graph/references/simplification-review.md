# Simplification review

Use one bounded, independent pass before the four delivery reviews to reduce unnecessary change.
Optimize for fewer concepts and less maintenance, not a line-count target. The Senior Engineer
remains the sole source/test writer; the simplification reviewer is read-only.

## Planning and dispatch

New delivery plans bind `simplification_review_version: 1` and `simplification_assignment` to the
plan digest. Present this assignment with the execution sequence before approval. It uses the
host catalog's review recommendation; task/plan v2 or v3 may override its model and effort through
`model_overrides.simplification_assignment`. Verify the exact approved pair and announce the
concrete agent name, scope, model, and effort before dispatch, as for other reviewers.

The Supervisor alone dispatches a fresh session using the existing `code_reviewer` profile with
`code_review_assignment: {"focus": "simplification", "required_skill": null}`. Supply the approved
brief, acceptance criteria, design when applicable, exact comparison/base, stable diff/checkpoint,
permitted read scope, and the task's inspection budget as its ceiling. Exclude writer rationale and
desired conclusions. Do not give this session ledger paths, operation IDs, or claim fences.
It may inspect only authorized source and evidence. It cannot edit, run checks, spawn helpers or
reviewers, create a reviewer-fanout request, mutate the ledger, publish, or expand scope.

This is an instruction-level host session, not a graph branch, helper-register member, or fifth
delivery-panel gate. The engine binds the assignment and preserves it on resume; it does not
enforce dispatch, report completion, or proposal dispositions. The Supervisor checks those duties
before recording the implementation result. Historical plans without the version marker gain no
new dispatch authority. Advisory and design-only work omit the pass.

## Review contract

Inspect the current change for unnecessary abstractions, speculative options or extension points,
redundant state, duplicated validation, wrappers with no useful boundary, unrelated cleanup, and
changes that can be removed while still meeting the approved acceptance criteria. Trace relevant
callers, contracts, and existing evidence before calling something redundant.

For each proposal, return a stable `SIMPL-` ID, exact file/line evidence, the concrete deletion or
simpler replacement, why it reduces complexity, the affected acceptance criteria, evidence that
required behavior survives, and the focused validation needed after the edit. State uncertainty
explicitly; incomplete evidence cannot justify a required deletion.

Do not seek unrelated refactors, compress clear code, weaken tests, remove necessary validation,
error handling, security checks, compatibility, or observability, or violate an actual architectural
boundary to save lines. Do not manufacture proposals when the diff is already simple.
Personal preferences remain optional. A required change needs demonstrated redundancy, unnecessary
scope, or a concrete maintenance problem traceable to the approved task and repository rules.
The Supervisor adjudicates that evidence; the reviewer has no veto or final acceptance authority.

Return these sections instead of the delivery panel's decision report:

- Review focus and exact reviewed checkpoint/comparison.
- Status: `COMPLETE`, `INCOMPLETE`, or `BLOCKED`, with any evidence or access gap.
- Reduction proposals: stable IDs, evidence, replacement/deletion, preservation argument,
  affected acceptance criteria, validation, and required versus optional classification.
- Retained complexity: relevant constructs that must remain and why.
- Skill usage: selected skills with provenance and concrete checks, or `None` with this contract
  and the checks performed. No broad review skill is required for this focused pass.

## Settlement and bounds

Pause writer edits at a stable checkpoint after its preliminary implementation handoff, while its
implementation attempt remains active. Run at most one simplification pass per implementation
generation, including a repair generation. This is a separate session from all four fresh panel
reviewers. Persist the report under the approved run artifact root through the Supervisor's normal
artifact authority, never by giving the reviewer write access.

The Supervisor rejects scope expansion and unsupported blockers, then sends one consolidated packet
to the same Senior Engineer. The writer records each proposal as accepted, rejected with evidence,
or deferred as optional, applies accepted reductions, and runs only affected validation. A required
proposal must be resolved by an edit or an evidence-backed rejection before recording success.
Unresolved design changes follow the existing DESIGN-BLOCKER/redesign path. Optional proposals
cannot prevent completion. Record the final implementation report, reduction report, dispositions,
and resulting checkpoint together before releasing the existing implementation dependency join.

Do not record implementation success early, create a second writer, rerun this pass to seek more
deletions, or add a new repair allowance. The writer's unchanged approved assignment and current
attempt cover this continuation. Preserve the attempt's lease and existing retry limits; an expired
or failed attempt follows the normal fenced recovery procedure. A retry does not renew this pass's
budget. Reuse its report only while the reviewed evidence remains applicable; otherwise report the
gap and use the existing recovery or replan path, never invent completion.

The four delivery reviewers, Test Engineer, and required specialists then inspect the final
checkpoint with fresh context. Their gates and the three-round delivery repair limit remain intact.
Changed code after those reviews must return to the affected gates. A later generation must not
repeat an evidence-backed rejected proposal unless its relevant evidence changes.

On resume, verify the recorded checkpoint, report, and dispositions before continuing. If the pass
was interrupted or its state cannot be established, report it as incomplete; do not silently
redispatch it under a new name to reset the limit. Track session identity, lifecycle, and usage
separately in run artifacts under `review_testing`; do not attribute its usage to the writer or
invent a ledger branch association. Disclose unavailable usage and engine-accounting gaps.
