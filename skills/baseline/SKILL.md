---
name: baseline
description: Get ready for the next task by returning to master or main, pulling the latest changes, and cleaning up verified merged task branches.
---

# Return to the default branch for the next task

Resolve the intended repository, remote and default branch. Inspect status and
registered worktrees. Fetch the remote and prune stale tracking refs. Use the
current checkout for the next task: switch it to the default branch when clean
and available. If another worktree already holds the default branch, use that
worktree for subsequent commands and give the user its path.
Fast-forward with `git pull --ff-only`. Preserve local changes and report any
checkout that could not be switched. Name the checkout used for subsequent work.

For each task branch, compare its current local and remote tips with the head
recorded for its merge into the default branch. For squash or rebase merges,
use the merged PR's head SHA. For direct merges, verify ancestry and the task
head recorded at merge time. Skip ambiguous tips, newer work, protected or
default branches, and branches being used by another active task.

Before removing a task worktree, verify its registered branch, resolved path,
HEAD and clean status. Check for ignored files that need preserving. Run
`git worktree remove` from outside that worktree. Preserve dirty worktrees.
Recheck each ref before deleting it. Use a lease against the verified head
for remote deletion so a concurrent push is preserved.

Verify the worktree directory and registration are gone, the deleted local
and remote refs are absent, and the default checkout matches the fetched tip.
Report the checkout path, revision, cleanup counts and any branches retained
because of local files, changed tips or missing merge evidence.
