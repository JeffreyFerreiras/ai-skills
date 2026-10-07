---
name: create-pull-request
description: Draft accurate GitHub pull request text or create a review-ready pull request. Use for PR drafting and explicit requests to create, open, submit, or publish a GitHub pull request.
---

# Create Pull Request

Write an accurate pull request from the selected changes. When creation is requested, open a GitHub PR that is ready for review unless `--draft` is passed.

## Gather Evidence

1. Read repository instructions, inspect status, and identify the intended changes. Exclude unrelated work.
2. Resolve the repository, base branch, head branch, and applicable pull-request template. Ask one focused question if any is materially ambiguous.
3. Base the title, body, and validation notes only on the selected diff and checks actually run. Do not invent results, links, motivations, or compatibility claims.

## Write the Pull Request

- Preserve the selected template. Without one, use **Summary**, **Testing**, and **Risks**.
- For an actual behavior change, explain the trigger and before/after result. Support it with available evidence, such as a failing/passing test, observed output, or screenshots for a visual change. Label reasoning from the diff as such; if a before run is unavailable, state that limit instead of inventing one. For documentation or behavior-preserving changes, describe the changed contract or structure without manufacturing a behavior comparison.
- Add a small diagram, diff sketch, or screenshot only when it makes the change easier to assess. Place it beside the explanation and retain only the relevant states or boundaries.
- Where risk matters, name the affected users, consumers, data, or services and the concrete failure they could face. Explain rollback difficulty when a revert alone would not restore the prior state, such as a destructive migration or external side effect. Tie risks to the selected change and evidence; avoid generic risk labels.
- Use the repository's title convention; otherwise use Conventional Commits, such as `feat: add caching`.
- "Write," "draft," or "prepare" a PR without a creation request produces text only.

## Creation Modes

- `$create-pull-request`, or an explicit request to create, open, submit, or publish a PR, creates a normal review-ready PR by default.
- `$create-pull-request --draft` creates one draft PR instead. The option overrides the default; never also create a normal PR.
- A plain-language request for a draft PR has the same draft state when it does not conflict with an option.

## Create Safely

1. Confirm `gh` is authenticated, the remote matches the repository, and no PR already exists for the head branch.
2. Create a focused branch and commit only the selected changes when needed. Push without force only for an explicit creation request.
3. Before creation, confirm the title and body still match the committed `base...head` range and check for secret exposure.
4. Run one `gh pr create` command with explicit repository, base, head, title, and `--body-file`; add `--draft` only for draft mode. Do not retry after an uncertain result.
5. Verify the PR's URL, base, head, title, body, and draft state with `gh pr view`. Remove the temporary body file in all outcomes.

Report the URL only after verification. Otherwise report the prepared text and the blocker.

## Source

Evidence, selective visuals, and concrete rollback/impact guidance are adapted from [mattpocock/skills pr](https://github.com/mattpocock/skills/blob/6fd947921b935b7e1e69293a200400f0fdd5c15f/skills/engineering/pr/SKILL.md), pinned at `6fd947921b935b7e1e69293a200400f0fdd5c15f`. Repository templates and the creation authority above take precedence over upstream's fixed body format.
