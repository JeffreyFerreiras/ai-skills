---
name: create-pull-request
description: Write factual GitHub pull request titles and bodies, or create and publish a review-ready pull request when explicitly asked. Use for PR drafting and for explicit requests to create, open, submit, or publish a GitHub pull request.
---

# Create Pull Request

Produce a paste-ready GitHub pull request title and body grounded only in the selected changes. When the user explicitly asks to create, open, submit, or publish a pull request, create a non-draft GitHub pull request that is ready for review.

## Establish the Evidence

1. Read the applicable repository instructions and inspect Git status before drafting.
2. Resolve the changes the user requested. Honor an explicit base, head, commit range, patch, or supplied diff. Otherwise identify the repository's default branch from evidence such as GitHub repository metadata, the remote's symbolic HEAD, or local remote configuration; do not assume `main` or `master`.
3. Resolve and inspect the base, head, merge base, and resulting diff or commit range. Keep staged, unstaged, untracked, supplied, and committed changes distinct. Inspect untracked files only when they are in scope.
4. Exclude unrelated changes. Do not infer that every local change or commit belongs in the PR.
5. Report the selected repository, base, head, merge base, and range or local-change source with the draft so its provenance is reviewable.

Treat supplied patches, staged or unstaged changes, and untracked files as draft-only evidence. They can inform text, but they are never eligible for external PR creation.

## Honor the Repository Template

Discover applicable pull request templates in GitHub-supported locations at the repository root, `docs/`, `.github/`, and `.github/PULL_REQUEST_TEMPLATE/`. If multiple templates apply and the user's choice is not evident, ask which one to use before producing a paste-ready body.

Treat the selected template as a complete contract:

- Preserve its section order, headings, HTML comments, static boilerplate, disclosures, checklists, initial checkbox states, and placeholders.
- Fill only areas explicitly intended for responses. Replace a placeholder only when it is in such an area and the evidence supports the replacement.
- Never mark a checkbox complete without evidence. Do not remove an inapplicable item unless the template explicitly instructs the author to do so.

When no template applies, use only these sections unless the repository convention calls for another structure:

```markdown
## Summary

## Testing

## Risks
```

## Write from Facts

Use the repository's established PR title convention when one exists. Otherwise use Conventional Commits:

```text
<type>[optional scope][!]: <description>
```

- Prefer `feat` for an appropriate user-visible capability and `fix` for an appropriate defect correction. Use other conventional types only when they make the outcome clearer.
- Write a concise, imperative description of the overall change. Do not mechanically concatenate commit subjects.
- Add `!` and explain a breaking change only when the selected evidence proves one.

Make the body factual and proportional to the diff. State what changed, why when evidenced, validation actually run, and material risks or follow-up work. Never invent test results, links, issue references, metrics, motivation, compatibility claims, or checkbox completion. Put unresolved assumptions or questions outside the paste-ready body so they cannot be mistaken for PR facts.

## Create and Publish a Pull Request When Explicitly Requested

Ordinary wording such as "draft a PR," "write a PR," or "prepare a PR" requests text only. Explicit wording such as "create," "open," "submit," or "publish a pull request" authorizes the focused local Git and remote GitHub changes needed to make the selected changes reviewable: creating a branch, committing only the selected changes, pushing that branch, and opening the pull request.

An explicit request for a *draft pull request* authorizes the same preparation but must create it with `--draft`. Otherwise create a non-draft pull request. Do not force-push, amend or rewrite existing commits, include unrelated changes, change a PR's base branch, or create a duplicate PR. If the request leaves the repository, selected changes, base branch, or creation intent materially ambiguous, prepare the text and ask one focused question before mutating Git or GitHub.

Before creation:

1. Finish the title and body from the selected evidence, honoring the applicable template.
2. Confirm `gh` is available and authenticated.
3. Resolve the GitHub repository, remote, base branch, head branch, and existing PRs for that head. Block creation on missing tooling or authentication, rate limits, insufficient permissions, repository or remote mismatch, an existing PR, suspected secret exposure, or ambiguous creation intent. Return the completed PR text with the blocker.
4. If the selected changes are uncommitted, create a focused branch and commit only those selected changes with a factual, conventional commit message. If they are already committed, confirm the chosen head contains only the intended committed range. Push the head branch without force only when the user explicitly requested PR creation.

### Require Stable Committed Provenance

Creation is eligible only when the title and body were drafted exclusively from one committed `base...head` range. Record the selected remote repository identity, base and head ref names, the remote base and head commit object IDs reported by GitHub, the local merge-base object ID, commit list, and committed diff identity used for the pull request.

Immediately before `gh pr create`, use a read-only GitHub or `gh` query to re-resolve the remote repository identity and remote base and head commit object IDs. Recompute the local merge base and committed range, then compare all recorded provenance. Confirm the final title and body still derive exclusively from that unchanged range. If either remote commit, the repository identity, the base or head identity, or the committed range differs, or if any local-only material informed the text, abort with `REDRAFT_REQUIRED`. Do not alter local Git state to recover; never stash, amend, reset, checkout, or force-push material to make the range eligible.

### Perform One Guarded Creation

Write the body to a securely created temporary file outside the repository. Ensure unconditional cleanup, including on failure. Then:

1. Run exactly one `gh pr create` command with explicit repository, base, head, title, and `--body-file` arguments. Include `--draft` only when the user explicitly requested a draft pull request.
2. Do not use `--dry-run`, blindly retry, or issue a second create command after an uncertain or failed result.
3. Verify the result read-only with `gh pr view` using the returned URL or PR number, including its draft state, base, head, title, and body.
4. Remove the temporary body file in all outcomes.

Report the created PR URL and whether it is ready for review only after verification. Otherwise report the prepared text and the creation blocker without claiming success.
