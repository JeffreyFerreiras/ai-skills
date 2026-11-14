# Production context and same-candidate review

Use this protocol when several production artifacts or reviewers depend on one
another. It tracks the production's evidence, not the system architecture taught
inside the video. Reuse the production document, claim ledger, narration/cues,
scene and bridge records, output hashes and Supervisor state. Keep private source
content in its authorized project. The helper checks declared structure and
provenance; independent reviewers still decide meaning and quality.

## Freeze declared dependencies

Create a versioned UTF-8 JSON manifest before the full candidate snapshot. Schema
1 has `production_id`, `stage_id`, nonnegative integer `generation`, `nodes` and
`gates`. Each node has a unique `id`, `kind`, nonempty unique `scope_ids`, unique
`depends_on` IDs and `artifacts`, each with project-relative `path` and lowercase
actual-byte `sha256`. Bind separate sections of one document to its whole-file
hash initially. The producer declares dependency completeness; independent review
checks it. A hash cannot discover omitted dependencies.

Typical branches are objective/example/script/storyboard, approved narration and
cues, timed source/assets, encoded master, decoded evidence/QA, then delivery.
Cover source/layout and actual crop previews join delivery when a cover is in the
brief. Declare shared style/source dependencies and the existing bridge record.
Omit optional work only through the frozen brief. Keep existing bridge schema,
digest, fixed visual/logical reviewers and exact-frame checks unchanged.

Each gate has `id`, `prerequisite_nodes`, `required` assignments, `check_ids`,
`consistency_coverage` and a `budget_checkpoint` artifact reference. Each required
assignment contains distinct `role`, distinct `reviewer_id`, `scope_ids` and
`criterion_ids`. Role IDs cannot contain `:`, which separates each role from its
local finding ID. Freeze the actual obligations from the existing deliverable gate;
the utility does not invent or waive roles. Optional `bounds` can lower
`packet_bytes` (65536), `evidence_refs` (128), `readers` (4), and `writers` (1).
Lower actual host limits still control dispatch.

The budget checkpoint is the trusted prior cumulative state, frozen before the
current candidate. The current reservation is separate, avoiding a hash cycle.
Budget checkpoint identity alone is excluded from scoped review-contract
comparison; all other gate semantics remain equal. Check current lineage separately.

Recursive node fingerprints bind stable IDs, kind, scopes, sorted artifacts and
dependency fingerprints. `contextDigest` binds all nodes, production/stage/generation
and frozen gates. Keep the existing full `candidateDigest` as the review identity.
Keep packets, reviews and mutable state outside the declared candidate and exclude
their paths explicitly. Never put a receipt inside its own candidate hash.

Limits are 256 nodes, 32 dependencies and 64 artifacts per node, and 262144 manifest
JSON bytes. Packet bounds include the complete transitive closure and validation
references. Oversize input blocks with a named error; split bounded in-scope work
explicitly, never truncate or silently expand resources. Duplicate JSON keys/IDs/edges,
unknown nodes, cycles, links/reparse points, unsafe paths, missing files, changed
hashes and unknown schemas fail closed.

## Prepare one frozen review round

Settle the one Writer, verify the live candidate and current prerequisite approvals
before packet creation. Script approval precedes any TTS/render; voice review
precedes final timing. A plan is not a completed prerequisite. Reserve the round
durably in existing Supervisor state. All roles share candidate/context/generation
and reservation. Use at most four read-only roles and one Writer, obey lower
observed capacity, and serialize scarce GPU, encoder, playback and provider work.
Never validate/review during Writer edits. The helper does not dispatch or reserve.

An explicit runtime gate index supplies `id`, `candidateDigest`, `reservation_id`,
`prerequisites`, `validation_evidence`, `prior_findings` and `final_consistency`.
Prerequisite JSON receipts bind production/stage/generation/candidate/context,
`node_id`, current `fingerprint`, `status: reviewed` and `decision: APPROVE`.
Check receipts bind the same identities plus gate/reservation, `check_id` and
`status: passed`. Every reference is checked against actual bytes. Missing scoped
carry proof requires fresh prerequisite evidence; a label never upgrades approval.

When encoded AAC QA applies, include frozen check ID `encoded-audio`. Its check
receipt adds `encoded_audio` with `encoded`, `approved_wav`, `settings`, `metadata`,
`diagnostics` and `listening`. The encoded ref must match a declared `encoded_master`
node. Metadata JSON binds `encoded_sha256` and `audio_codec: aac`; diagnostic JSON
binds that encode, `input_wav_sha256`, canonical `settingsDigest`, measured finite
`peak_dbfs`/`true_peak_dbfs` and integer `clipped_samples`. Listening JSON binds the
encode, `reviewer_id`, `coverage: full_encoded_audio` and `decision: APPROVE`.
All three use schema 1 and actual path/hash refs. These structural links do not
measure audio or prove that a listener actually heard it; existing target/quality
checks and truthful independent review remain necessary.

Technical packets contain assigned scope, complete dependency closure, relevant
criteria, actual validation evidence, stable prior finding IDs and bounds. Exclude
Writer reasoning and desired verdicts. Record actual supplied packet path/hash,
counts and inspection coverage.

Logical review starts in a fresh session with an allowlisted audience packet:
opaque IDs, actual encoded/frame/audio refs named `audience/NUMBER.ext`, neutral
numeric intervals and audience-observation questions. Withhold intent-bearing
IDs/paths, script, planned outcome, implementation, prior interpretations and Writer
reasoning. Runtime `audience` contains only `media` and `intervals`; media refs
contain only `path` and `sha256`. JSON cannot prove the host supplied no other context.

Persist the first reading before constructing its technical comparison packet.
It binds gate/candidate/context/reservation, `reviewer_id`, audience `packet` ref,
`input_scope: audience_evidence_only` and nonempty `observations`. Then save a
separate `host_capture` receipt with the same bindings, actual `packet`,
`available_inputs` containing only that packet, `fresh_session_id`,
`first_reading_sha256`, and increasing integer `first_reading_sequence` and
`intent_release_sequence`. Supply both refs in the runtime gate. Missing host
evidence remains an isolation gap. Only the technical packet releases intent and
relevant prior findings. Other roles also receive no Writer reasoning or desired verdict.

## Fan in once, repair once

Accept one terminal result per required role/scope from its expected distinct
non-author identity. Results echo schema, production/stage/generation,
gate/reservation, candidate/context digests, actual technical `packet` ref,
`role`, `reviewer_id`, sorted `scope_ids`/`criterion_ids`, reviewed `closure`,
`status: reviewed`, `decision: APPROVE|REVISE|BLOCK`, `findings`, `dispositions`
and artifact `evidence`. Each finding has a role-local stable `id`, `criterion_id`,
`scope_id`, `locator`, observed `issue`, `effect` and bounded `repair`.

Namespace findings by role, retain cross-role attribution, and sort by criterion,
scope, role, ID and locator. Arrival order or majority voting cannot settle
contradictions. Every prior ID requires a role-owned `resolved`, `still-open`,
`reopened` or `blocked` disposition; omission remains open. Return all findings to
the same Writer in one repair packet. Missing, duplicate, conflicting or cross-version
evidence blocks aggregation. Any BLOCK blocks; REVISE/open findings require repair.
APPROVE needs all roles, current checks and fresh final consistency. Empty new
findings alone cannot close a gate. Negative or pending final consistency preserves
the consolidated REVISE/BLOCK repair and unresolved IDs. Pending consistency may
omit the receipt, or supply a current bound receipt with `status: pending` and no
decision. A reviewed consistency BLOCK also blocks the repair outcome. None of
these cases permits APPROVE advancement.

Changed/removed nodes and dependencies invalidate descendants through the union
of old/new edges, even when descendant bytes match. Keep historical evidence
immutable and report stale IDs. Whole-candidate approval never carries automatically.

A scoped carry receipt uses `status: carried_forward`, current bindings and exact
role/scope/criteria, `prior_receipt` path/hash, `old_candidateDigest`,
`old_contextDigest`, complete `old_closure`/`new_closure`, origin reviewer and
`unresolved_findings: []`. Origin must be a valid scoped approval with actual
packet/coverage, matching packet/result/frozen-assignment reviewer identities,
no findings and no unresolved IDs. Closure membership,
fingerprints, relationships, scopes and review semantics must match. Changed
ancestors, removed edges, expanded scope or criteria reject reuse. Current result
still binds current packet/reservation; runtime `previous_context` comes from
Supervisor's preserved validated context. This preserves origin approval without
claiming a fresh review. Never carry final-encode, delivery or final-consistency acceptance.

Before advancement require fresh final-consistency evidence binding current
gate/candidate/context/reservation, `status: reviewed`, `decision: APPROVE`,
`fresh: true`, recomputed `live_candidateDigest`, exact frozen `coverage` and
artifact `evidence`. Recheck live bytes/context and full candidate through the
existing snapshot workflow. Cover narration/captions/visual timing, encode and
delivery path as applicable. Structural proof cannot establish semantics,
dependency completeness, actual independence or truthful reviewer assertions.

## Retain cumulative bounds and honest status

Budget schema 1 contains production/stage identity, stable `lineage_id`, increasing
`sequence`, cumulative `spent_rounds`, `policy_limit`, `allowance_limit`, immutable
`allowance_refs`, ordered `reservations` and `attempts`. Reservations have unique
`id`, consecutive `round`, `candidateDigest` and `contextDigest`. Current state
extends trusted prior state without dropping/changing reservations or raising
limits. Effective cap is the smaller limit, at most three rounds. Initial review
is round one; all roles share it. Explicit allowance never erases spent work.
Equivalent work cannot reset by renaming stage/output. Reconcile unknown/pending/
active attempts in their original reservation before new dependent dispatch.
Attempts have unique `id`, `reservation_id` and reconciled `status` (`settled` or
`not-started`); a previously uncertain attempt retains its identity and a checked
`reconciliation` artifact ref. Never erase an uncertain dispatch from the lineage.
Supervisor selects latest trustworthy state and checks origin authority. At spent
round three, unresolved IDs block dependent production; never start round four.

Progress states active action, next action and whether input is required. Keep
planned, active, validated, reviewed, delivered and accepted states distinct.
Record measured wall elapsed, active production, review, waiting and observed
capacity separately, allowing overlap and unknown values. Do not sum concurrent
durations or claim an unmeasured speedup. Unaffected read-only planning may continue.

## Standalone helper

Resolve the script from the installed skill; Python 3.10+ and stdlib suffice:

```text
python -B SKILL_DIR/scripts/production_context.py check context.json --project PROJECT
python -B SKILL_DIR/scripts/production_context.py packet context.json --project PROJECT --gate GATE --role ROLE --phase technical --evidence index.json --output new-packet.json
python -B SKILL_DIR/scripts/production_context.py aggregate context.json --project PROJECT --gate GATE --evidence index.json --output new-repair.json
```

Evidence index schema 1 contains runtime `gate`; aggregation also names exact
`packets`, `results`, current `budget` and `previous_budget` artifact refs. No
globs, media jobs, network, models or provider calls occur. Outputs use exclusive
new paths and cannot overwrite history. Partial/uncertain writes remain unknown
until reconciled. Errors have `code`, `locator`, `message`; invalid/pending evidence
exits nonzero. No automatic migration: absent records leave existing manual duties.

Apply [focused preflight](production-preflight.md) and the existing
[teaching-opening criterion](teaching-opening.md). NotebookLM uses these boundaries
only for accessible artifacts; inaccessible vendor internals remain gaps. Do not
put local renderer/voice commands into its generation packet or claim local checks
inspected unavailable vendor output.
