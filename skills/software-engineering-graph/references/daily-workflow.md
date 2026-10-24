# Daily workflow recovery and rehearsal

`status` retains `next_action.kind=blocked` for a terminal block. Its `recovery` field,
also returned as `blocked_recovery` by `recovery show`, identifies the block reason,
original rejecting gates, immutable ledger result references, and successor-plan requirements.
A successful review report with a `BLOCK` decision is a rejection, even though the worker
completed its assignment. It must remain visible when no failed branch repair packets exist.
These diagnostics are read-only and grant no recovery authority.

`DESIGN_GATE_BLOCKED`, `CONSOLIDATION_BLOCK`, security blocks, policy denials, and exhausted
budgets remain terminal. Preserve that run, resolve the finding, and propose a separately
approved successor plan with its required independent review and security gates. The existing
format-6 and format-7 missing-review-skill equivalence recovery remains narrowly scoped; it does not
reopen a rejected design. Format-7 repairable evidence and bounded design revisions retain
their existing transitions and budgets.

For an audited format-7 equivalent review, retain the failed result, original source binding, prior
inputs, capabilities, and check receipts. The source must still match the review boundary. Recovery
creates a fresh fenced attempt for only that failed review and leaves architecture independently
required. If delivery then needs an in-scope repair, the next writer and all reviewers use the new
generation. Replace checks with their exact selected predecessors. A new missing-skill failure needs
new branch-specific authorization; earlier equivalent-skill approval does not transfer or reset an
allowance. Status, resume, and replay retain these bindings.

Task scope text describes intended work; it grants no filesystem capability. A task can narrow
a policy directory ceiling such as `repo:src/` to exact approved files. Workers receive those
file targets, not the directory ceiling. Effect and action must match; commands and external
capabilities retain exact target matching. Parent paths, sibling prefixes, wildcard or hashed
targets, traversal, sensitive/custom denied paths, and link or reparse escapes remain forbidden.
Planning and resume recheck actual repository paths. A retry of an approved file grant cannot
restore a broader policy directory grant. A two-file successor cannot add `repo:scripts/` or a
broad repository write because its first file was authorized under `repo:src/`. `POLICY_AUTHORITY_EXCEEDED` requires
a smaller authorized proposal or an explicitly authorized policy change and new plan approval.
Never rewrite a ledger or broaden policy as an automatic recovery step.

New execution plans bind `capability_projection_version=1` into the approved canonical
plan digest. This interpretation permits filesystem containment within policy and role
ceilings while retaining the approved target's original spelling. OS-wide case folding
only selects candidate matches. Initialization and semantic status, resume, and dispatch
validation require filesystem identity proof (`os.path.samefile`) for every case-only
task/role-ceiling pair. The corresponding ceiling prefix must exist and refer to the
same object; distinct case-sensitive siblings, missing prefixes, or unavailable proof
fail closed. Exact literal grants can still name new files or directories. Commands
and external capabilities still match exactly. Plans without
the marker retain their historical exact capability intersection, including conditional
reviewer assignments, during status, resume, dispatch, and immutable envelope validation.
Reconstruction does not add a marker, change approval hashes, or rewrite stored envelopes
or task metadata. Unknown or malformed versions fail closed. A historical run needs a
separately approved new plan to use containment; resume cannot silently upgrade it.

Resolve actual calendar dates before approval for a window that crosses midnight. For example,
an October 20 evening window ending at 02:00 ends on October 21 in the selected timezone.
Keep a two-hour elapsed-time deadline separate from that calendar window. This engine change
does not add a scheduler or date parser.

## Merge evidence contract

`graph_engine.merge_readiness.assess_merge_readiness` is a pure evaluator of a supplied evidence
snapshot. It requires graph completion, an approved review for the exact current full commit
SHA, no unresolved review threads, CI for that same SHA, and `success` for every named required
check. Missing, pending, failed, cancelled, skipped, neutral, and unknown checks block readiness.
An empty required-check list is invalid. A new PR head invalidates prior review and CI evidence,
even when the tree is unchanged. Refresh both before reevaluating.

The caller must fetch current authoritative repository, PR, review, and CI evidence, including
the complete required-check list and thread state. The supplied values are not authenticated
by this evaluator. Recheck the exact head at any separately authorized merge boundary; do not
use a cached readiness result to merge a later head. Branch protection, authority, and required
checks still apply. This module has no provider adapter or merge command.

The only ready decision is `DRY_RUN_READY`. Every result sets `merge_authorized=false` and
`deploy_authorized=false`; requesting live execution returns `LIVE_EXECUTION_UNSUPPORTED`.
A live daily workflow still needs its separately authorized regular PR publication, exact-head
CI and review verification, and validated reversible development deployment. Local simulation
does not prove those remote actions or actual model reviews succeeded.

## Executable rehearsal

Run from the skill directory with bytecode disabled:

```text
python -m unittest -v tests.test_daily_workflow
```

One continuous CLI trace preserves a terminal rejected design in `RUN-1`, carries its original
rejection evidence into an explicitly approved corrected `RUN-2` successor, and completes all
fresh design, security, four code-review focuses, test, and consolidation gates. Both runs retain
the same policy digest and exact two-file source grants, with no broad source or artifact writes.
The successor fixes and locally tests the Friday May 8, 2026 service: a 00:30 slot after a 19:00
opening resolves to May 9. A meaningful local Python unit command runs through fenced writer
and independent test-executor attempts before bound acceptance and `complete`. The completed
snapshot reaches `DRY_RUN_READY`, with merge and deployment authority false. The original
terminal run and rejection bytes remain unchanged. No revision/repair allowance was consumed
before the terminal rejection, and the explicitly approved successor keeps those limits.
The test emits one `DAILY_SUCCESSOR_TRACE` JSON summary with fixture provenance, hashes,
exact grants, fresh gates, local check outcome, completion, and the dry-run decision.

Separate regressions exercise bounded design revision, terminal rejection diagnostics, denied scope
expansion, the existing audited equivalent-review recovery, format-7 source changes during
review, independent review-panel slots, delivery repair, three local required checks, bound
acceptance evidence, and graph completion. An unchanged tree at a new disposable Git commit
then fails the real review-source freshness gate. Pure merge scenarios exercise missing,
failed, pending and stale-head evidence. Fixtures and worker reports are synthetic; no live
models, Out2Nite repository, remote CI, merge, deployment, or automation is touched.

`tests.test_graph_hardening.GraphHardeningTests.test_format7_equivalent_review_recovery_across_delivery_repair`
adds one continuous disposable trace: exact two-file grants, a real local consistency check, audited
missing-skill recovery, all mandatory reviews, an in-scope delivery repair, replacement checks, fresh
generation-specific recovery, independent test execution, bound acceptance, and `complete` on the
repaired source. It checks stale source/fences/predecessors and mismatched authorizations fail closed,
and retains old failure and receipt bytes. `FORMAT7_RECOVERY_TRACE` labels its role reports synthetic
and records final source hashes, retry counts, repair usage, and preserved-artifact hashes.
