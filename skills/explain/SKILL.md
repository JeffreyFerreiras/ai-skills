---
name: explain
description: Explain a diff, commit, or branch from intent through validation, with a Mermaid sequence diagram. Use for change walkthroughs, not findings-first reviews or terse summaries.
---

# Explain Changes

Build an evidence-backed mental model of the change. Explain the observable implementation approach and rationale without claiming access to hidden chain-of-thought.

## Establish the Change

1. Read the applicable repository instructions and inspect repository status.
2. Use the range, commit, branch, pull request, or patch named by the user. For a pull request, prefer connected forge read APIs; otherwise use the forge CLI's read-only metadata and diff commands. For a remote branch that is not already available locally, prefer a forge read API. Do not check out branches, fetch refs, or otherwise mutate repository state merely to explain a change.
3. If the user names no scope, use current staged and unstaged changes. When the working tree is clean, resolve the remote default branch from `refs/remotes/origin/HEAD`, with the repository's established default branch as a fallback. Explain the current branch relative to that default when they differ; otherwise explain the latest commit. State the selected scope explicitly. Ask the user only when no meaningful scope can be resolved or materially different plausible scopes remain.
4. Establish intent from the user's request and available task, commit, or pull-request description. Keep stated intent distinct from conclusions inferred from the code.
5. Inspect the complete diff plus enough surrounding implementation, callers, tests, configuration, and contracts to explain the affected behavior. Trace important symbols beyond the changed lines when necessary.
6. Identify the previous behavior, the new behavior, the entry point, participating components, important data or state transitions, failure paths, and validation evidence.

This is an explanatory workflow. Do not edit files, recommend unrelated redesigns, or turn the response into a defect review unless the user also asks for those actions. If a correctness concern prevents an accurate walkthrough, state it as a bounded caveat.

## Explain the Approach

Describe the approach at the level a technical collaborator can verify:

- the problem or constraint the change addresses
- the implementation strategy and where responsibility now lives
- the main decisions visible in the change and their practical consequences
- meaningful tradeoffs or alternatives documented in the task or made evident by the code
- how tests or checks exercise the new behavior

When explaining an agent's or model's approach, summarize decisions and evidence from the request, edits, tool results, and artifacts. Do not present private reasoning, hidden chain-of-thought, or an invented chronology. Label unsupported but useful conclusions as `Inference` and explain the evidence behind them. Use `Unknown` when the rationale cannot be established.

## Sequence Diagram

Always include at least one Mermaid `sequenceDiagram` that shows the changed path end to end.

- Use real actors and component names from the code when available.
- Start at the external trigger and end at the observable result or persisted state.
- Show important calls, returned values, state changes, and boundary crossings.
- Use `alt`, `opt`, or `loop` only for branches that materially affect understanding.
- Mark the newly introduced or changed interaction with a short Mermaid note.
- Keep unchanged context only when it helps explain where the change fits.
- Prefer one readable diagram. Split it by scenario only when a single diagram would obscure distinct success and failure flows.

If the change has no runtime behavior, diagram the relevant build, generation, migration, or authoring flow and label it `Non-runtime flow`. Never invent a runtime interaction merely to satisfy the diagram requirement.

## Response Shape

Lead with the behavioral outcome, then explain in this order:

1. **What changed:** Scope, intent, and the user-visible or system-visible outcome.
2. **Before and after:** The old path compared with the new path. Omit the old path only when it cannot be established.
3. **Sequence diagram:** The changed behavior in execution order, followed by a short prose reading guide.
4. **Implementation approach:** Responsibilities, key decisions, rationale or labeled inference, and important tradeoffs.
5. **Code map:** Group files by responsibility and link to the most relevant local lines. Use clickable local file links with exact line numbers when the interface supports them; otherwise use `path:line`. Explain each group's role rather than reciting every changed file.
6. **Validation:** State exactly what automated checks, tests, or manual evidence exist and what behavior each proves. Distinguish executed evidence from tests merely present in the diff.
7. **Limits and risk:** List only material unknowns, untested paths, compatibility concerns, or assumptions that affect confidence.

Scale the depth to the change. For a broad change, organize the walkthrough into coherent behaviors and cover each one; do not compress important state transitions into a file list. Define unfamiliar domain terms on first use, keep terminology consistent, and place warnings beside the behavior they qualify.
