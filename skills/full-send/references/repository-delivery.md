# Repository delivery

Use this reference for repository publication and PR delivery under the invoked
full-send or full-stop mode. Its limits follow every delegated stage.

## Prepare the PR

Resolve the exact repository, remote, base, task branch, and existing PR before
publication. Commit only task changes. Follow the project's branch-update method.
Before rebasing, fetch and record the expected remote task head. Never overwrite
newer work. Use an explicit expected-head lease for an authorized force push.

Bring the branch up to date before final proof and review. A rebase, conflict fix,
or changed base invalidates affected evidence; verify the resulting target.
Reuse passing evidence only when its relevant inputs remain unchanged.

Invoke `create-pull-request` to create or update the one task PR. The invoking
mode supplies scoped publication authority; full stop still forbids merging.
Follow the repository template and existing-PR checks. Have `writing-loop`
accept the exact title and body before posting them. Later prose edits need new
acceptance. Scoped PR preparation covers metadata writes; comments, messages,
and thread resolution need separate user intent.

When the graph owns publication, its Pull Request Engineer performs the approved
Git and PR mutations. The coordinator does not also commit or push. Do not use
`git-push` to publish a subset of a dirty checkout.

## Monitor and repair

Invoke `babysit-pr` with the exact PR, endpoint, review requirements, remaining
repair budgets, and publication limits. It reports events; the delivery owner
routes fixes to the existing writer and reviews the new candidate.
Use `address-pr-feedback` with all actionable in-scope feedback selected by this
delivery request. Draft explanations for rejected findings; do not post or resolve
threads without the corresponding permission.

Code repairs return through focused checks and the code-review route. Prose
repairs return through `writing-loop`. Preserve issue identities and consumed
repairs across pushes and monitoring restarts. An unexpected remote-head change
requires reconciliation, not an automatic overwrite.

## Live merge gate

Immediately before readiness or merge, fetch the base and read the live PR.
Bind the result to its exact head and base commits. Confirm all of these:

- The PR is open, not a draft, on the intended base and task branch.
- Current proof and applicable reviews cover the final candidate and relevant
  base changes. Every material in-scope finding has an evidence-backed disposition.
  No required thread or review remains unresolved.
- Required CI checks pass for that head. Skipped, missing, pending, or
  inconclusive required results are not passes.
- Required reviewers and configured review bots have finished on the current
  candidate. No human request for changes remains active, and no required
  approval is missing.
  Do not invent a bot requirement when the project has none.
- The branch is up to date, conflicts are resolved, and live mergeability is known.
  Unknown mergeability or changed inputs require another read or verification.

For full stop, positively verify that neither auto-merge nor a merge queue can
merge the PR without the user. An unavailable field is not proof of disabled
automatic merging.

## Complete the selected endpoint

Full send merges only after the gate passes, using the allowed method and
protection rules. Match the reviewed head at the merge boundary, such as
`gh pr merge --match-head-commit <sha>`. Never use `--admin` or a protection
bypass. If using an authorized queue or auto-merge, wait for actual merged state
within the monitoring budget.

Verify live merged state and the merge commit before reporting completion.
Follow active task-worktree cleanup instructions after an authorized merge.
Verify the intended resolved path, clean worktree, and recorded branch heads
before removal. Preserve dirty worktrees and newer branch work; report blocked
cleanup rather than force-deleting it.

Full stop never performs the merge step. Use its ready, waiting-for-approval,
or blocked handoff and leave the PR open and unmerged.
