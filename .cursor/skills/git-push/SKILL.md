---
name: git-push
description: Commit and push all pending changes in the current Git repository when the user explicitly requests it, including changes made by others.
---

# Git Push

Use this skill only when the user explicitly asks to commit and push all pending changes. That
request authorizes including tracked and untracked changes regardless of who made them; do not
discard or selectively omit another person's work because of its origin.

Keep this a stage, commit, and push operation. Do not run tests, builds, linting, code reviews,
secret scans, hygiene checks, or other validation unless the user explicitly requests them.
Do not expand the task based on the types of files changed.

1. Run `git add -A` to stage all changes.
2. Read only enough staged diff information to write an accurate commit message, then run
   `git commit -m "<message>"`. If there is nothing to commit, continue to the push.
3. Run `git push` for the current branch using its configured upstream, without force.
   If a command fails, report the blocker; do not start a repair or validation workflow.
4. Briefly report the commit hash, branch, and push result.
