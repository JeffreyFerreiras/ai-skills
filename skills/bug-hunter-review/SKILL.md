---
name: bug-hunter-review
description: Review a specified code change for concrete introduced bugs. Use for a dedicated bug-hunter review, not naming-only or architecture-only feedback.
---

# Bug Hunter Review

Find actionable defects introduced by the assigned change. Review in this agent; do not delegate,
edit files, switch branches, stash changes, commit, publish comments, or repair findings.

## Review target

Use the supplied repository, comparison, scope, and checkpoint. For a graph assignment, the
approved brief and review envelope control the target and permitted reads. Inspect the complete
assigned diff and relevant callers, tests, contracts, and configuration within that scope.

For a base-branch comparison, review the diff from the merge base, including working-tree changes
only when they belong to the requested target. Do not replace an explicit comparison with the
repository's default branch. Report missing diff metadata or inaccessible evidence as a gap.

## Find demonstrated failures

Trace changed behavior from realistic inputs and existing contracts. Check boundary conditions,
error and cancellation paths, state transitions, concurrency, and resource ownership. Follow
affected callers far enough to establish whether the changed behavior produces a failure.

Use existing tests and validation logs as evidence. Do not run tests or add reproduction files
without explicit authority. A passing test does not cover an untested path; missing tests alone
do not prove a bug.

Report a finding only when the change introduces a discrete defect and the triggering conditions
can be demonstrated from the code or permitted evidence. Explain the failing scenario, impact,
and smallest fix condition. Continue through the remaining diff after finding an issue.

Exclude pre-existing bugs, intentional behavior changes, hypothetical inputs outside the contract,
naming preferences, and architecture or pattern suggestions without a concrete failure. If the
expected behavior is unresolved, report that uncertainty instead of inventing a requirement.

## Return the review

Use the assigned graph report format when provided. Otherwise, lead with findings ordered by
severity, each with the changed file and line, triggering conditions, impact, and focused fix.
Distinguish critical failures, urgent defects, ordinary defects, and low-impact defects.

Use stable `BUG-` finding IDs for graph reviews. If there are no demonstrated bugs, say so.
Report unreviewed areas and material evidence gaps. Do not approve a review that could not inspect
its assigned target. Keep the result separate from the writer's rationale and other reviewers'
conclusions.
