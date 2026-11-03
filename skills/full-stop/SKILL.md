---
name: full-stop
description: Complete an explicitly requested full-stop workflow and leave the reviewed result ready for the user's final action. For PRs, leave them open and unmerged. Incidental emphasis, quotations, and questions about the skill do not invoke it.
---

# Full stop

Start only for an intentional full-stop request or `$full-stop`. A closing
"full stop" used as emphasis does not invoke it. Asking about the skill does not
authorize work.

Read the installed `full-send` skill from the host catalog or a known sibling
installation. Follow its scope, code/prose routing, proof, reviews, repair limits,
and repository delivery instructions. These overrides take precedence.
The request authorizes the same scoped preparation and task-branch publication,
including lease-protected task-branch rewrites, but grants no merge permission.

## Prevent automatic merging

Never merge, enable auto-merge, enter a merge queue, push to the default branch,
bypass branch protection, or delegate those actions. Propagate this constraint
to every role, including the graph's approved publication owner.

For an existing PR, verify both auto-merge and merge-queue state before edits or
pushes. This mode authorizes disabling auto-merge and removing this PR from its
queue. Verify the result and report those changes. Check again before each push
and handoff. If state cannot be inspected or automatic merging cannot be prevented,
stop PR mutations and report the blocker.

## Handoff at the gate

For a PR, apply full-send's live merge gate as a readiness test. Leave it open,
not a draft, and unmerged, with no automatic merge scheduled. If someone else
merged or closed it, report that observed state instead of claiming a handoff.

- **Ready to merge:** every gate passes on the live head and base.
- **Waiting for required approval:** only a confirmed human approval remains.
- **Blocked:** another required gate, evidence gap, or repair limit prevents readiness.

Stop and unregister an owned monitor at handoff. Report the PR URL, final head,
checks and review outcomes, exact verdict, and remaining risks. Confirm whether
the PR is open and unmerged. Do not perform post-merge cleanup for an unmerged PR.

For prose outside repository delivery, leave the writing-loop-accepted artifact
ready for the user's action, in the requested review location. Acceptance does
not authorize sending or publishing it. State any factual or contract blocker.
Do not invent a PR requirement.

## Source

Adapted from [Theo Browne's full-stop](https://github.com/t3dotgg/tstack/blob/1085050f3baed1ac724caa4a8166145fb2ac88af/skills/full-stop/SKILL.md),
pinned at `1085050f3baed1ac724caa4a8166145fb2ac88af`.
License: [MIT](LICENSE.txt).
