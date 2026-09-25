---
name: git-push
description: Commit and push all pending changes in the current Git repository when the user explicitly requests it, including changes made by others.
---

# Git Push

Use this skill only when the user explicitly asks to commit and push all pending changes. That
request authorizes including tracked and untracked changes regardless of who made them; do not
discard or selectively omit another person's work because of its origin.

1. Check the repository, current branch, upstream, and full status. Review the pending file list for
   secrets or files that repository instructions forbid tracking. Resolve a concrete issue before
   staging; do not expose sensitive content in output.
2. Stage all eligible changes with `git add -A`. Review the staged file list and run
   `git diff --cached --check`. Use relevant project checks when the pending changes warrant them.
3. Write a commit message that accurately summarizes the combined changes, then commit. Push the
   current branch to its configured upstream without force. If the push is rejected, inspect the
   reason and resolve it without overwriting remote work.
4. Report the commit hash, branch, push result, and any changes that could not be included.
