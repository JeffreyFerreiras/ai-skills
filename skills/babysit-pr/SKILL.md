---
name: babysit-pr
description: Watch one GitHub PR through checks and reviews. Use when asked to watch, monitor, or babysit a PR, or when full-send or full-stop delegates monitoring.
---

# Babysit a PR

Watch one PR until it is ready, needs action you cannot take, closes, merges,
or reaches the deadline. Standalone monitoring reads and reports. It grants no
authority to edit, push, change PR state, post comments, or resolve threads.

When a delivery owner delegates monitoring, keep its endpoint, current writer,
review requirements, publication limits, and remaining repair budgets. Send
events to that owner. Full-stop leaves the PR open and unmerged. Full-send owns
the authorized merge and verifies its result.

## Read the live PR

Resolve the exact GitHub host, repository, and PR number from the request or
the current branch PR. Verify `gh auth status` for that host before network CLI
work. Use the explicit PR number and `--repo <owner/repo>` on each PR command.
Include the host in the repository selector for a different GitHub host.

Use `gh pr view` with JSON fields such as `state`, `isDraft`, `headRefOid`,
`baseRefName`, `baseRefOid`, `mergeable`, `mergeStateStatus`, `reviewDecision`,
`reviewRequests`, and `autoMergeRequest`. Confirm the intended branches and
read queue membership when the endpoint depends on it. Use
`gh pr checks <number> --repo <owner/repo> --required` and the repository's
applicable rules and delivery requirements to identify and verify required
checks.

Read thread state for the resolved host, owner, repository, and PR number. This
command uses PowerShell or Bash quoting. Replace `HOST`, `OWNER`, `REPO`, and
`NUMBER` with the resolved values.

```text
gh api graphql --hostname HOST --paginate -f owner=OWNER -f name=REPO -F number=NUMBER -f 'query=query($owner: String!, $name: String!, $number: Int!, $endCursor: String) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      reviewThreads(first: 50, after: $endCursor) {
        nodes { id isResolved isOutdated comments(first: 1) { nodes { url } } }
        pageInfo { hasNextPage endCursor }
      }
    }
  }
}'
```

Read all pages within the monitoring deadline. Use the first comment URL to
identify each thread and read its discussion when deciding what needs action.
A failed or incomplete query leaves thread state unknown. A flat comment list
cannot establish thread resolution.

Use the repository's actual approval and review bot requirements. If no bot is
required, CI can meet the checks gate without a bot response. Verify any required
bot's result for the current candidate. An optional bot can still raise an
actionable finding. Verify findings against the source and keep outstanding
findings across pushes. Outdated thread locations do not resolve threads.
If required rules or state are unavailable, report what you could not verify.

## Wait with one watcher

Default to 30 minutes for the whole monitoring window. Set one deadline at the
start and keep it across pushes, retries, and resumes. Read the live state before
starting a watch. Only the task responsible for monitoring starts one. Record
the repository, PR, process handle, deadline, and latest head and base in task
state. Reconcile any existing watch before starting another. Never stop another
task's process.

While required CI is pending, run this in a yielding host process:

```text
gh pr checks <number> --repo <owner/repo> --required --watch --fail-fast --interval 30
```

Supervise that process within the deadline. Read review state and PR identity
at intervals of at least 30 seconds, even while CI is pending. Keep these reads
bounded by the same deadline. Yield between reads so user input can interrupt.
Use the host's process controls to stop the owned watch at deadline or handoff
and verify it stopped. If a watch exits, times out, or loses its handle, reconcile
its status before replacing it. If you cannot establish that it stopped, report
the remaining watcher as a blocker. Ending a turn does not schedule a wakeup.

Treat watch output or exit as a reason to read live state. In standalone mode,
accept a required check that reports `skipped` or `neutral` on the current
candidate when GitHub accepts that conclusion. A workflow that never ran or
never reported the required check leaves that check missing. Missing, pending,
or inconclusive required results cannot satisfy the gate. For delegated
monitoring, apply the delivery owner's stricter gate where it differs. The
repository delivery gate for full-send and full-stop requires passing results
and excludes skipped required checks. When no checks are configured, evaluate
the other gates.

Honor GitHub's retry or reset time, and retry a transient read failure at most
twice within the deadline. Report authentication failures or required state
that remains unreadable. Keep credentials out of reports.

## Decide and report

- A closed or merged PR ends the watch. Report the live state and the merge
  commit if available. A draft needs action before it can be ready.
- A changed head or base invalidates affected check and review evidence. Stop
  and settle any watch for the old candidate. Read the new candidate before
  restarting. Report unexpected remote changes to the delivery owner before
  any repair or push.
- Failed required checks, conflicts, an out-of-date branch, active changes
  requested, or unresolved required threads block readiness. Report the failed
  check or thread links. The delivery owner routes authorized CI and branch
  repairs to the existing writer. For authorized, selected review repairs, the
  owner uses `address-pr-feedback` with the exact PR URL. Verify that its
  inspection method can target that PR before it runs. If it cannot, report
  the blocker. Verify findings before editing and keep fixes within scope.
  Report required threads that stay open. Draft a reason for dismissing a
  finding. Posting that reason and resolving its thread need separate user
  authorization.
- After an authorized repair and push, reread the PR and verify the updated
  candidate within the original deadline and remaining repair budgets. If the
  budget runs out, report the blocker even if CI is green.
- If only required human approval remains, hand off to the delivery owner or
  user as waiting for approval. At the deadline, report each gate that is still
  pending or blocked.

Report ready only when the PR is open and out of draft, required CI and reviews
meet their gates, required threads are settled, and live mergeability confirms an
up-to-date branch without conflicts. Verify that the live head and base still
match the commits covered by the evidence. Repeat affected reads if they change.
For full-stop, the delivery owner must also verify that auto-merge is off and
the PR is out of the merge queue before handoff. For full-send, the delivery
owner checks its live merge gate immediately before merging.

Stop and confirm the owned watcher has ended before handing off or asking a
question. Give the PR URL, final head and base, observed gates, and next action
if one remains. Report an unknown gate or watcher state explicitly.

## Source

Adapted from [Theo Browne's babysit-pr](https://github.com/t3dotgg/tstack/blob/11e37a74d0a6868c121cd0910ab2b3bd9873b92a/skills/babysit-pr/SKILL.md).
This version uses native GitHub CLI monitoring and repository-specific review
requirements. See the [MIT license](LICENSE.txt).
