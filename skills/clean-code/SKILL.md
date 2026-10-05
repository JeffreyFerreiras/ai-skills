---
name: clean-code
description: Write or refactor readable, maintainable production code. Use for focused naming, function design, duplication, or Clean Code improvements.
---

# Clean Code

## Workflow

1. Inspect the surrounding code, tests, conventions, and current behavior before editing.
2. Identify the concrete readability, design, safety, or maintainability problem. Avoid broad rewrites without a demonstrated benefit.
3. If that problem exposes material design pressure or a SOLID concern, use the optional Design Graph Check before implementation.
4. Make the smallest cohesive change that improves the code while preserving public behavior unless a behavior change is requested.
5. Verify with the narrowest relevant tests, lint, type checks, or build commands.
6. Summarize the meaningful improvement and any remaining risk.

## Design Graph Check

Use this optional lookup only for material, demonstrated design pressure or SOLID concerns in the requested work. Skip trivial local changes, mechanical edits, documentation-only work, and generated code. Do not search merely to find a pattern to apply.

1. Describe the pressure in domain terms: the affected responsibility, observed axis of change, and current correctness or maintenance cost. Start with evidence in the code or requested behavior, not a desired pattern name.
2. Resolve this skill's directory from the loaded `SKILL.md`, then run:

   ```text
   python <skill-directory>/scripts/search_review_graph.py "<observed pressure>" --depth 1 --max-nodes 8 --json
   ```

   Use the project's configured Python interpreter when available. If Python is unavailable, read `references/review-graph.manifest.json` and apply the same one-hop, typed-edge lookup manually. Search at most three distinct material pressures across the entire task, including repair passes. Keep depth at 1 and results at no more than 8 nodes.
3. Treat each match as a hypothesis. For a pattern, verify its intent, applicability, tradeoffs, and `avoid_when` conditions against the actual code and expected change. For a SOLID principle, verify its concrete cues and guardrails, and identify the affected contract or demonstrated cost. Keywords and similar class shapes alone do not justify a change.
4. Choose the simplest design that addresses the demonstrated pressure, including no pattern when direct code is clearer. Reject indirection for hypothetical reuse, stable one-off branches, or complexity that a pattern only moves elsewhere. Briefly explain a material design choice and its tradeoff when summarizing the work.

The bundled graph covers SRP, OCP, LSP, ISP, and DIP. Search by the observed pressure, a principle's full name, or its acronym when relevant; no full-principle audit is required for writing. Both resources are self-contained in this skill. The `review` filenames and schema names remain for compatibility with the review skill's existing search interface.

## Design Guidance

- Use intention-revealing names consistent with the language and codebase.
- Keep functions and classes cohesive; extract code when it separates responsibilities or levels of abstraction, not to satisfy a line-count rule.
- Make side effects and dependencies explicit. Introduce interfaces only for real boundaries, variation, or useful test seams.
- Reduce duplication when it represents shared knowledge. Do not abstract coincidentally similar code prematurely.
- Keep error handling appropriate to the language and contract. Preserve useful context and handle failures at a deliberate boundary.
- Prefer simple control flow and data shapes over clever compression.
- Keep comments for rationale, constraints, or non-obvious behavior; remove comments that merely narrate the code.
- Treat tests as behavior documentation. Avoid over-mocking and assertions tied to incidental implementation details.

## Pragmatic Limits

- Follow local architecture unless the requested change specifically addresses an architectural problem.
- Do not require strict TDD retroactively. Add or update tests in proportion to behavioral risk.
- Do not force arbitrary limits on function length, parameter count, assertions, or class size.
- Do not turn straightforward code into layers of pass-through abstractions.
- Do not alter unrelated code merely because it could also be cleaner.

## Review Versus Implementation

For review-only requests, report evidence-backed findings without editing. For implementation requests, make the change and validate it. If the request is ambiguous, prefer diagnosis before mutation.
