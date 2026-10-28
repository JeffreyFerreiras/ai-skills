# Per-run persistence

Small JSON findings record for one panel run. The Supervisor owns and writes it.
Reviewers never write it. Keep the record and worker logs outside the installed
skill and unrelated source, for example a disposable run directory.

## What the record holds

- Frozen review identity: workspace, base, head or PR head, review revision,
  allowed scope, and sha256 hashes of mutable inputs (diff, logs, generated files).
- Raw initial reports, stored once per role and never rewritten.
- Findings with stable ids (`F1`, `F2`, ...), origin role, evidence, disposition
  (`proposed`, `accepted`, `rejected`, `unresolved`, `out-of-scope`) plus reason,
  reviewer replies with stance (`agree`, `withdraw`, `dispute`, `note`),
  fix and verification status, staleness flag, and append-only history.
- Follow-up requests (`Q1`, `Q2`, ...) with role, finding ids, source snapshot,
  and status (`pending`, `replied`, `unresolved`). A request is persisted before
  its packet goes out, so a crash never loses pending-follow-up status.

## Phase gate

1. `initial`: the Supervisor collects one report per role, or marks a missing
   role `unknown` with a reason. No dispositions, replies, fix verification,
   or follow-up requests are possible in this phase; the guards reject them so
   blind collection cannot be decided early. First-pass packets carry identity
   and scope only, never another reviewer's output, even when a record already
   holds some reports. Build them with `first-packet` during this phase only:
   after lock, on close, or on a stale source the helper refuses new dispatch
   packets.
2. `lock`: allowed only when every role is `reported` or explicitly `unknown`.
   Unknown or silent roles stay uncovered; they never become approval.
3. `reconciling`: dispositions, replies, fix notes, and follow-up requests are
   allowed. Each request is recorded with `request-followup` before its packet
   is emitted with `followup-packet`; a packet carries only that request's
   finding ids with their evidence, disposition, and reason. A reviewer may
   agree, withdraw, or dispute. A dispute reopens its finding to `unresolved`
   with the objection as reason, so disagreement stays visible. A requested
   follow-up stays open until a linked reply arrives or the Supervisor marks it
   `unresolved` with a reason via `resolve-request`. Pending requests block
   `verified` fixes and `close`; an explicit unresolved gap with a reason
   unblocks them. Only `close` ends the run, and only with a fresh source and
   no pending requests.

An `unknown` role never flips silently to covered. Before the lock, the host
may `reopen-role` with a reason so a late report can be accepted; the original
unknown reason stays in history. Reopening needs host settlement first: confirm
the old worker stopped and inspect workspace state, never run a duplicate while
the first outcome is unknown. The helper keeps no process engine and settles
nothing itself. After the lock, only a fresh run can revisit role outcomes.

## Source changes

Recompute the declared mutable-input hashes and compare the current workspace,
base, head, review revision, and scope with `check-source` before accepting or
reusing results. The host supplies the current values; the helper never calls
GitHub and never runs model processes. The command needs at least one identity
value and exits nonzero on mismatch, so a host script cannot mistake a stale
target for success. Use `--input-file NAME=PATH` to hash evidence files here
instead of pasting digests.

Any difference invalidates live decisions on the old target: `mark-stale`
marks every finding stale, including `rejected` and `out-of-scope`, and resets
`fixed` or `verified` fixes to `open`, while evidence, stable ids, and history
stay intact. A stale record refuses new dispositions, replies, fix updates,
follow-up requests, packets, and closing. Display of old evidence is still
allowed; reuse as current is not. Start a new verified run for the new target
(`init` accepts `--supersedes` to link the old run id) and re-review it.

## Helper

The [helper script](../scripts/review_state.py) uses the standard library only. It validates
records strictly: findings are a list with unique stable ids, a monotonic
counter above existing ids, known roles, known dispositions and fix states,
reasons for non-proposed verdicts, and append-only history. Malformed records
fail clearly and are never overwritten as fresh runs. Writes validate first,
then replace the file atomically (temp file plus rename), so a failed write
leaves the previous valid record readable. Rename guards against partial
writes, not power loss or concurrent writers: run one Supervisor per file.

```powershell
# Synthetic sequence on a disposable record. Collect an initial outcome
# (report or explicit unknown) for every required role before lock.
python skills/review-panel/scripts/review_state.py --state C:\tmp\panel-run.json init --run-id run-1 --workspace C:\repo --base abc --head def --input diff=<sha256> --scope src/
python skills/review-panel/scripts/review_state.py --state C:\tmp\panel-run.json add-report --role logic --text "..."
python skills/review-panel/scripts/review_state.py --state C:\tmp\panel-run.json first-packet --role logic
python skills/review-panel/scripts/review_state.py --state C:\tmp\panel-run.json lock
python skills/review-panel/scripts/review_state.py --state C:\tmp\panel-run.json add-finding --origin-role logic --title "..." --evidence "{\"file\": \"a.py:1\"}"
python skills/review-panel/scripts/review_state.py --state C:\tmp\panel-run.json request-followup --role logic --id F1 --reason "second look"
python skills/review-panel/scripts/review_state.py --state C:\tmp\panel-run.json followup-packet --request Q1
python skills/review-panel/scripts/review_state.py --state C:\tmp\panel-run.json add-reply --id F1 --role logic --stance dispute --text "..." --request Q1
python skills/review-panel/scripts/review_state.py --state C:\tmp\panel-run.json check-source --head def --input diff=<sha256> --scope src/
# Alternative for a request that is STILL PENDING (no reply arrived):
python skills/review-panel/scripts/review_state.py --state C:\tmp\panel-run.json resolve-request --request Q1 --outcome unresolved --reason "reviewer silent"
```

## Cooperative limits

Prompt limits are cooperative instructions, not sandbox enforcement. Atomic file
replacement guards against partial writes, not against permissions or
multi-writer races. Run exactly one Supervisor per record file; the helper does
not implement transactions. State is a recovery aid, not a security boundary.
No shared chat or cross-task vault state is involved; each record belongs to
one run.
