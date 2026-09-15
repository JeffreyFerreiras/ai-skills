# Ledger operations

Read the essential invariants below, then the section for the command or route being used. `<skill>`
denotes the installed skill root; schema paths in CLI instructions are relative to that root. The
entry skill controls scope and authority.

## Essential ledger invariants

- The Supervisor is the sole ledger and `graphctl` mutator. Branches receive only claimed envelopes,
  never database paths, operation IDs, sibling claims, or ledger control metadata.
- No branch executes before explicit approval of the immutable execution plan. Every result,
  heartbeat, timeout, and other attempt-scoped mutation presents the current attempt ID and claim
  token. Stale fences fail closed.
- Budgets, retry ceilings, mandatory gates, and resource assessments remain authoritative. A retry,
  replacement, restart, or renamed task never refunds or resets consumed allowance.
- Put input manifests only in the derived run inbox and use unique operation IDs. Identical requests
  replay; changed content under the same identity conflicts.
- After interruption, `resume` preserves valid state and requires every running attempt to settle
  from its actual result or an explicit fenced timeout. Never infer completion from a lost session.
- Windows permission and durability acknowledgments disclose platform limits; they grant no
  authority and cannot weaken approval, fencing, or evidence requirements.

Read `Start a run` for initialization, plan approval, fixed research fan-out, and platform
acknowledgments. Read `Instruction-level helpers` or `Optional reviewer delegation` before those
paths. Read `Operate the ledger` before claim, record, join, retry, resume, abort, or completion.
Read `Token accounting at phase handoffs` before binding, collecting, closing, or reporting usage.

## Start a run

Before substantive scoping, capture the primary session's usage checkpoint when the host exposes
the metadata and the explicit file is authorized to read:

`python <skill>/scripts/graphctl.py usage checkpoint --session-log <explicit-file>`

This read-only preflight runs without `--repo`, policy, initialization, or any ledger write. Retain
only the returned `checkpoint_schema_version: 1`, hashed `source_id`, `offset`, and `prefix_sha256`.
If no metadata is available, say that token usage is unavailable and continue the approved workflow.

Before the first stateful command, select one absolute state root for the shell and retain it for
every later stateful command in the run:

```powershell
$env:SOFTWARE_ENGINEERING_GRAPH_STATE_HOME = 'C:\graph-state'
```

An explicit `--state-root` may be used instead, but it must be repeated unchanged on every stateful
command. The stateless usage checkpoint above is exempt.

1. Inspect the worktree and create a redacted, immutable task brief matching
   [the task-brief schema](task-brief.schema.json) under a repository-policy artifact root.
2. Hash the exact `.codex/engineering-graph.json` bytes and put that digest in the brief's
   `policy_approval`.
3. Initialize the ledger and generate the execution-plan summary. Pass `--size small|medium|large`
   when the Supervisor chooses an explicit size; otherwise the engine records its bounded recommendation:

  `python <skill>/scripts/graphctl.py --repo <repo> [degraded acknowledgments] init --run-id <id> --task-brief <path> --size <size> [--host codex|codex-astra|cursor] --op-id <id>`

4. Present the returned `execution_plan` and its digest to the human. Record an explicit local approval
   or rejection before dispatching anything:

   `python <skill>/scripts/graphctl.py --repo <repo> record plan-approval --run-id <id> --plan-digest <digest> --decision APPROVE --authority-ref authority:<id> --op-id <id>`

   `next`, `ready`, and `next --claim` remain blocked while this approval is pending. A rejected plan
   blocks the run; start a new run for a materially different size, route, role set, model, or effort.
5. Dispatch only the envelope returned by `next --claim` after approval. The first branch is always
   `impact_mapper`.
   When `status` reports `record_fanout_assessment`, record one complete, evidence-backed Supervisor
   assessment before claiming any sibling:

   `python <skill>/scripts/graphctl.py --repo <repo> record fanout-assessment --run-id <id> --fanout-id <id> --assessment-manifest <path> --authority-ref authority:<id> --op-id <id>`

   Cover every listed member and all four resource categories. Order every exclusive conflict and any
   service usage needed to keep each unordered set within capacity. The assessment is immutable.
   For `design_only` and `full_delivery`, the Impact Mapper result creates the fixed assessed-pending
   `design_research_architecture` and `design_research_validation` fan-out before any Tech Lead is
   created. Both branches reuse the Impact Mapper role at the approved host economy assignment,
   receive deterministic architecture/validation focus and split inspection budgets, and may project
   only filesystem or external read capabilities. They must return a verified `evidence_manifest` with evidence, no
   decision, and no findings. Sealing `research_collection` materializes the canonical evidence and
   creates the same-generation Tech Lead. A failed exhausted mandatory research pair blocks the run.
6. Put each returned branch manifest in the derived run inbox shown by `init` or `status`, then use
   `record branch-result` with the claimed `attempt_id` and `claim_token`. Branch manifests never
   contain control mutations.

### Instruction-level helpers

Read [Economy helpers](economy-helpers.md) for the canonical allowance, eligibility, host discovery,
lifecycle, and accounting contract, and load the selected helper profile's behavior contract.
Evidence Scout reuses direct evidence children; Validation Executor is a separate mechanical
command contract for Senior Engineer and Test Engineer. Both may directly invoke either approved
helper. Other eligible parents may invoke only Evidence Scout.

New deterministic registration requires a task/plan v3 allowance attachment. The Supervisor hashes
the allowance first, obtains approval for the plan that binds its reference/hash, and initializes the
separate run-bound helper register. Parents receive only its path and redacted context, never ledger
control metadata. The register validates requests against the immutable allowance, its approved
parent capability ceilings, observed host support, shared limits, resources, and current checkpoint.
Unchanged calls within an allowance need no per-call Supervisor dispatch or approval.

Task/plan v1 and v2 have no authority through the new register. Loading new source does not rewrite
or revoke a historical run's separately approved instruction-level contract. Exact assignments and
actual host restrictions must be verified. Helper results cannot replace required ledger check
receipts or parent judgments.

### Optional reviewer delegation

Delegation is disabled unless both the repository policy and task brief provide
`reviewer_delegation`. An enabled execution-plan v2 lists every conditional assignment and its exact
role, model, effort, lens, prompt template, reason/acceptance/evidence/scope ceilings, derived
read-only capabilities, instance limit, and dispatch weight. Human approval covers these values.

A primary Code Reviewer may return `review_preliminary` plus `review_fanout_request`; it never
dispatches engine-managed reviewer-fanout children. It may spawn a direct evidence child only under
the separate contract above. The Supervisor records the reviewer request with the live attempt fence:

`python <skill>/scripts/graphctl.py --repo <repo> record review-fanout --run-id <id> --branch-id <id> --attempt-id <id> --claim-token <token> --preliminary-manifest <path> --request-manifest <path> --authority-ref authority:<id> --op-id <id>`

When status requests it, the Supervisor records the read-only resource assessment:

`python <skill>/scripts/graphctl.py --repo <repo> record review-fanout-assessment --run-id <id> --request-slot-id <id> --assessment-manifest <path> --authority-ref authority:<id> --op-id <id>`

The engine permits depth 1, at most 3 children per request, 6 children and weighted cost 15 per run,
and at most 2 request rounds. The default round ceiling is 1. Effective values are the minimum of
engine, repository, task, and approved-plan ceilings. Child failures, timeouts, and skips stay in the
nested collection and never refund cost. Once every member settles, the parent becomes ready with a
fresh claim fence and a redacted continuation that cumulatively binds every slot/collection digest,
exact member tuple, terminal non-success, and finding source without ledger or operation metadata.

On Windows, pass `--ack-degraded-permissions` because Python cannot prove profile DACL exclusivity.
Pass `--ack-degraded-durability` only when directory sync is genuinely unavailable and the reported
degraded mode is acceptable. These flags acknowledge platform limitations; they grant no authority.

## Operate the ledger

### Repairable evidence, explicit format 7

New runs keep format 6 until the active Supervisor executes:

```text
evidence enable --run-id RUN --contract-version 2 --coverage-manifest INBOX_JSON --op-id ID
```

The bounded inbox manifest is `{"schema_version":1,"kind":"check_coverage","checks":[{"check_id":"CHECK","relevant_inputs":["src/"],"complete":true}]}`.
Include every required check, at most 32 checks and 16 exact/subtree input paths per check. The
Supervisor attests that these paths cover the configured command's inputs; the engine cannot infer
dynamic dependencies from argv. Declarations must fit existing task read authority. Actual snapshots
cover the task/executor read-root intersection, and must cover every declared relevant input.
Enablement preserves historical bytes and approvals. It tolerates stale legacy source freshness,
but retains all artifact, command, producer, authority and ledger integrity checks.

In format 7, `check run` also requires `--executor-branch-id`, `--executor-attempt-id`,
`--executor-claim-token`, `--source-branch-id`, `--source-attempt-id`, and `--source-claim-digest`.
The executor's actual secret token authorizes execution; a public digest cannot replace it. Keep
tokens out of transcripts and evidence. The Supervisor supplies them through the existing claim
transport. Source provenance does not require possessing the writer's secret. Replacing a selected
receipt requires its exact `--replace-ref`; history remains immutable.

The first check on each actual executor/check attempt is included. Each repeat on that same attempt
uses one existing delivery-repair unit, or a design-revision unit on a design-only route. A new
operation ID, source edit or session cannot create a free slot. A normal repaired generation already
consumes its existing repair unit and gives its fresh writer/tester attempts their own initial slots.
There is no second allowance system and no refund for failed or abandoned reservations.

The engine commits a reservation before execution and records the receipt afterwards. A completed
operation replays without executing; an unfinished reservation reports `CHECK_EXECUTION_UNKNOWN`.
After establishing that the original process has stopped, the Supervisor may use
`check abandon --reservation-id ID --reason TEXT --run-id RUN --op-id ID`. This records an
attestation; it does not kill a process. A new execution then requires a new operation and its repeat
debit. Do not replay an unknown operation to rerun it.

Snapshots include tracked/index identities, deletions and nonignored untracked content within
approved roots. Explicit exact targets also cover ignored files. Bounds are 10,000 paths, 16 MiB per
file, 128 MiB content, 2 MiB Git enumeration output and 30 seconds per snapshot. Links, reparse points,
submodules, sensitive files, incomplete coverage and unavailable Git fail closed. Before/after
digests detect source changes during checks. They cannot detect malicious change-and-restore;
retain an exclusive validation window. A PASS exit alone does not make a receipt eligible.

At delivery dependency advancement, the engine captures one source binding for all mandatory review
members. Final check receipts and acceptance wrappers must match that binding. Preliminary writer
checks remain history and cannot substitute for checks bound to the final review collection. Changed
source cannot reuse old approvals, even with fresh passing checks. Failure/non-approving results
remain recordable for running reviews; use existing repair, block or new-task handling otherwise.

Finalize handoffs and source inputs before the review boundary. Prefer already-permitted staging
outside covered source or canonical ledger results. For host-persisted reports under covered artifact
roots, `join advance` accepts `--generated-output-plan INBOX_JSON`. Its payload has `schema_version:1`,
`kind:generated_output_plan`, and at most 32 `outputs`, each with exact `path`, `purpose`,
`producer_node_key`, and `artifact_kind`. Purposes are `review_report`, `consolidation`, and
`acceptance_wrapper`. The last two belong to `supervisor_delivery_consolidation`.

Only absent, untracked exact files within both task write scope and permitted artifact roots qualify.
Implementation roots, source artifact references and declared check inputs cannot be excluded.
The host persists returned reports; producer identity indicates report ownership and grants no
filesystem capability to read-only roles. No directory exclusion or new write authority follows.
An existing output must be registered by its actual producer before check capture. Optional outputs
may remain absent. Registered output changes fail ordinary artifact integrity.

A declared consolidation file, when used, must contain the exact submitted inbox result manifest.
An acceptance output is the staged acceptance evidence file sealed by the engine's acceptance wrapper.
`record acceptance-evidence --replace-ref REF` replaces a prior wrapper explicitly. All required
checks and the current accepting collection/consolidation must already be eligible.

### Recovery and deterministic drafts

`recovery show --run-id RUN [--branch-id ID]` reconstructs bounded ordinary failure packets from
persisted attempts/results. `record retry` may accept `--repair-manifest INBOX_JSON` containing
`schema_version:1`, `kind:repair_judgment`, `cause`, `failed_criterion_ids`, and `authorized_scope`
(existing capability objects). Causes are implementation, design, dependency, infrastructure or
unknown. Criteria must belong to the task and scope must remain within the failed role's authority.
Missing judgments stay explicit; unavailable source-change summaries are not proof of no change.
Retry retains the failed result reference and claim refreshes the packet without changing its authority.

`consolidation draft --run-id RUN --join-id COLLECTION_ID` uses the same source/finding reducers as
validation. It returns a manifest bound to the sealed collection digest. Delegated issues requiring
judgment remain unresolved and cannot manufacture ACCEPT. The Supervisor still claims, inspects,
records and advances through every existing gate. Neither read-only command executes or retries work.

### Future-run planning constraints

Use `constraints select --bundle PATH --context PATH` before task finalization. Both bounded JSON
files must be inside existing policy artifact roots. Context contains `repository_id`,
`acceptance_ids`, `role`, and `scope_paths`. This scoping view shows candidates without activating them.

A bundle is `planning_constraints` version 1 with at most 32 records. Each record has `id`, positive
`revision`, `statement`, `rationale`, `state`, `repository_id`, up to 16 exact/subtree `paths`, `roles`,
`acceptance_ids`, `finding_id`, `confirmed_finding_ref`, `accepted_fix_ref`, `authority_ref`,
`invalidation_reason` (nullable), and `supersedes` (ID:revision strings). Supporting finding/fix JSON
must identify the same finding; accepted-fix evidence has `status:accepted`. States are candidate,
active, invalidated or superseded. Missing support, conflicting revisions and non-applicability
exclude a record; supersession cycles reject the bundle.

To activate a chosen bundle, include it in task `evidence_paths` and add the exact constraint marker
`planning-constraints:v1:BUNDLE_SHA256` before ordinary plan approval. Active authority references
must match the task's approved policy authority. Enable format 7 before any role claim. The full task
digest binds this choice without changing historical authoritative task projections.

Tech Lead and sole writer receive applicable active records only when immutable task scope identifies
unambiguous `repo:` paths, without unresolved exclusions. Read capabilities and shared review coverage
do not establish assigned work scope. Other unverified assignments yield explicit unresolved-scope
diagnostics and no active records. Claim inputs contain a filtered projection, never an embedded unfiltered
bundle. The full immutable task remains available by reference. Invalidation/supersession apply to
future bundles/tasks only; there is no live revoke, registry, history scan or automatic reopen action.
Changing an approved bundle fails immutable-input checks and requires existing new-task/plan handling.

Declared known check inputs must appear in bounded source acquisition. An ignored file covered only
by broad directory read authority causes `SOURCE_UNVERIFIABLE` before execution; declaring it does not
grant an exact read. Already authorized exact file targets remain covered, including ignored content.

When source changes during reviews, the Supervisor can still claim delivery consolidation to return
REPAIR, REDESIGN, or BLOCK. Immutable binding integrity remains required. Positive review results,
ACCEPT, acceptance evidence, and completion still require fresh source.

A sealed delivery REPAIR creates a compact packet for the next sole writer and exposes it through
`recovery show`. It binds the originating collection, consolidation, report findings/evidence/attempts,
permitted writer scope, return gates, and remaining cumulative allowances. Unknown cause and criteria
remain explicit. Claim refresh preserves the original evidence while updating revision and allowances.
Check execution requests persist only token digests; this statement does not describe the existing
generic claim-operation response store, which retains the returned claim response.

- Use `ready` or `next --all` to inspect dispatchable branches. Use `next --claim --op-id <id>` to
  claim exactly one branch atomically.
- Multi-member fixed review fan-outs begin pending. After the Supervisor assessment, independent roots
  become ready together and ordered successors promote atomically only after predecessors settle.
  Retryable failure does not release a successor. Do not use this mechanism as an arbitrary DAG scheduler.
- Use `join validate` before `join advance`. Collection joins only freeze terminal branch results
  and activate a typed Supervisor consolidation branch. Consolidation joins alone apply precedence,
  consume loop budgets, block, or activate the next generation.
- Use the typed `record timeout`, `skip`, `retry`, `heartbeat`, `approval`, `budget-use`, and
  `acceptance-evidence` commands for Supervisor mutations. Timeout, result, and heartbeat mutations
  must present the current attempt fence. Use `check run` for a policy-configured local command;
  required checks are satisfied only by its ledger receipt, not by a user-authored PASS file.
- Read consolidation inputs only from the claimed envelope. Its canonical `collection` input embeds
  every frozen branch result and terminal status, so consolidation branches never need ledger or
  database access.
- Give every mutation a unique opaque operation ID. An identical replay is a no-op; changed input
  under the same ID is an operation conflict.
- Use `resume` after interruption. Resolve every running branch by ingesting its actual result or
  recording an explicit timeout with its current attempt fence. Expired leases appear as a timeout
  action; send `record heartbeat` before expiry when work is still active.
- Use `complete` only after the closure join, acceptance evidence, approvals, and required checks
  are satisfied. Use `abort` for rollback; retained databases are audit evidence and are not deleted.

`status --json` is the supported export. Treat it as sensitive operational metadata.
It also reports schema-6 attempt counts and deterministic UTC wall-clock timing. Retry waits count toward
branch lifecycle wall time but not active duration or critical-path weight.

## Token accounting at phase handoffs

After initialization, bind the preflight checkpoint to the primary scoping phase:

`python <skill>/scripts/graphctl.py --repo <repo> record usage --run-id <run> --action bind --session-log <file> --phase scoping --generation 0 --start-offset <offset> --source-id <digest> --prefix-sha256 <digest> --op-id <id>`

Supply all three historical fields together. Identity, byte-prefix digest, and snapshot boundary
must match exactly; a mismatch leaves the ledger unchanged. Omit all three only to begin a new
baseline at the latest validated cumulative snapshot. A late baseline does not recover earlier
phases. Offset zero counts the first total only when the validated last usage equals the cumulative
total; otherwise the unknown prefix stays excluded and coverage is partial.

For an executed branch, replace `--phase` and `--generation` with the exact `--branch-id` and
`--attempt-id`. The engine derives role, phase, and generation from the attempt. Bind resumed
sessions separately to that same attempt; retries use their distinct attempt IDs. Bind delegated
    reviewers separately, never copy a child's usage into its parent. Helper sessions are
    not engine branches and remain in the separate helper register with separate usage accounting. A single
source cannot have overlapping bound intervals within the run. Do not share counted source intervals
across runs; the engine never searches other runs or sessions to discover ownership.

Collect while work continues and close at a settled checkpoint:

`python <skill>/scripts/graphctl.py --repo <repo> record usage --run-id <run> --action collect --binding-id <binding> --session-log <file> --op-id <id>`

`python <skill>/scripts/graphctl.py --repo <repo> record usage --run-id <run> --action close --binding-id <binding> --session-log <file> --op-id <id>`

Close the current primary phase before binding the next using the returned checkpoint, even when
both phases share a turn. Use `scoping`, `research_design`, `implementation`, `review_testing`, and
`closure`, with the applicable generation. Repeated cumulative snapshots and repeated collections
do not add tokens. The same normalized request/op ID replays its original result; new samples need
new operation IDs. Accounting mutations remain available in initialized, active, blocked, complete,
and aborted runs, including late closure metadata, without changing those states or delivery gates.

At every major phase handoff, report `usage.observed_totals`, the current phase, cumulative run
usage, role/agent usage, and observed model/effort pairs from `status`. Missing telemetry must be
reported as unavailable. Do not infer actual models from execution-plan assignments. Cached input
and reasoning output are subsets, cache writes are separate, and the headline is input plus output.
Read `coverage`, null `complete_totals`, `metric_partial`, missing executed attempts/primary phases,
open bindings, and unattributed increments before presenting any total as complete. Unexecuted or
skipped work has no measured consumption; active runs and unfinished responses are provisional.

Ambiguous in-run increments are counted once without guessed phase/model/effort attribution.
Increments that may include pre-run history are excluded. Resets drop the uncertain bridge while
retaining earlier observations. Identity changes, rewrites, and truncation retain a disclosed gap
and require an explicit new binding to replace the source. The final report can itself add tokens
beyond its last checkpoint. `complete` describes only bounded closed intervals, not all host work.

Missing executed retries keep the relevant role, agent, and generation summaries partial, and
missing primary phases keep Supervisor summaries partial. Model/effort groups cover observed
intervals only and expose association coverage; never assign missing usage to a planned model.
The source's `cache_write_input_tokens` is normalized to `cache_write_tokens` in reports and takes
precedence over the legacy source alias, even when invalid. Source counters retain signed 64-bit
bounds; exact aggregate integers can exceed that bound and require integer-preserving consumers.
Rewritten files retaining the same session ID cannot bind earlier overlapping offsets. Recovery
requires a new source identity or a later nonoverlapping checkpoint.

Only explicit authorized regular files are read, with 64 MiB/file, 1 MiB/read or record,
100,000 records/invocation, and depth-24 JSON limits. Symlink/reparse components and changed opened
file identity are rejected. Opens also request nonblocking mode where supported to prevent a raced
FIFO replacement from hanging before the opened-file type check. Source IDs and consumed prefixes
are hashed; raw source paths, conversation content, credentials, and arbitrary source strings never enter usage events or
responses. Errors are fixed diagnostics. Provenance is a caller association, not host authenticity.
See [Observed token usage](../README.md#observed-token-usage) for the supported observed JSONL
shape and report fields. If the host format or metadata is unavailable, keep coverage honest and
continue the authorized graph workflow without adding an approval gate.
