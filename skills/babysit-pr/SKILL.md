---
name: babysit-pr
description: Monitor an identified PR's checks and reviews when asked to watch or babysit it, or when an invoked full-send/full-stop workflow delegates monitoring. Report readiness and route authorized repairs without granting merge or messaging permission.
---

# Babysit a PR

Observe one identified PR until readiness, a concrete blocker, or the monitoring
deadline. A standalone watch request grants read access and reporting only.
It does not authorize edits, pushes, merge, comments, or thread resolution.

For delegated monitoring, retain the delivery endpoint, allowed mutations,
writer, review requirements, and remaining repair budgets. Full-stop's no-merge
constraint follows every action. The delivery owner routes authorized repairs;
the monitor does not create another writer or reset budgets.

## Read the current state

Resolve the exact repository and PR. Prefer suitable connected GitHub tools;
use authenticated `gh` for missing fields or watch support. Check `gh auth status`
before network CLI work. Stop for invalid credentials; never expose them.

Read the live head and base, open/draft state, checks, review decision, required
approvals, mergeability, auto-merge and queue state, and unresolved threads.
Use `address-pr-feedback` for thread-aware inspection when feedback needs action.
Flat comments do not establish thread resolution. Derive requirements from
repository rules or the delivery owner's explicit contract. Do not guess bot
requirements. Report unavailable required state as an evidence gap.

Bind findings and proof to head and base identities. Old check success or review
receipts do not establish readiness after a push. Preserve relevant older
findings until explicitly settled; an outdated thread is not automatically resolved.

## Wait without duplicate watchers

Use one owned watcher per PR. Record its handle, endpoint, deadline, and latest
observed head so a resume can reconcile it. Never duplicate a watcher with an
unknown outcome or cancel another task's watcher.

Default to a 30-minute window unless the user or approved plan specifies another
limit. Keep its deadline across retries and restarts in this invocation.
For CI, use `gh pr checks <number> --repo <owner/repo> --watch --fail-fast --interval 30`
through a yielding host process. Check success is an event, not PR readiness.
Supervise the process against the same deadline; stop the owned watch when the
window expires and verify it stopped. Inspect reviews and the live candidate
after it returns. No configured checks
does not establish that other gates passed.

Use host notifications when they support the missing review events. Otherwise,
make bounded state reads at intervals of at least 30 seconds, adjusted to the
service's retry/reset guidance and remaining window. Yield between reads so user
input and useful progress updates remain possible. Do not assume T3 tools exist
or that ending the turn schedules a wakeup.

On rate limits, honor the retry/reset time. Retry a transient read failure at most
twice. If reset is beyond the deadline or required state remains unreadable, report
the blocker. Timeout or a lost process response is unknown; reconcile the old
execution before retrying.

## Handle an event

- On a changed head or base, tell the delivery owner which proof and reviews need
  rechecking. Reconcile unexpected remote work before further mutation.
- On failed checks or actionable findings, return evidence to the authorized
  repair owner. For read-only watching, report the needed action and stop.
- If only required human approval remains, report that confirmed state and stop.
- If the PR is closed or already merged, report the observed state and stop.
- At the deadline, report pending gates and the final head as waiting or blocked,
  never ready. Do not silently start a new window.

After an authorized repair and push, reuse the same deadline and remaining
repair budgets. Reinspect the candidate before waiting again. Repair exhaustion
is a blocker even when CI is green.

## Finish

For delegated monitoring, return current evidence to the delivery owner's
full-send/full-stop gate. Standalone watching reports readiness only when live
checks, required reviews, threads, and mergeability support it; it never merges.
Stop and unregister the owned watcher at handoff or a question, settling uncertain
execution first. Report the PR URL, final head/base, observed gates, pending
actions, and any remaining watcher. Never invent review or merge evidence.
