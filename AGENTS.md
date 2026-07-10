# Global Agent Instructions

## Instruction Scope

- Treat this file as profile-level guidance that applies across projects.
- Read the nearest repository or directory-level `AGENTS.md` before acting. More specific local instructions override this file when they conflict.
- Follow the user's explicit request and preserve existing project conventions.

## Search And Discovery

- Use `rg` for fast file discovery and text search when it is available.
- Prefer native IDE or agent search tools when they provide more precise file, symbol, or semantic search.
- Inspect relevant files, callers, tests, configuration, and repository status before making changes.

## Planning And Communication

- Start with a short plan for multi-step, ambiguous, risky, or externally visible work. Skip ceremony for trivial tasks.
- State material assumptions and risks early while continuing with safe, reversible work.
- Lead final responses with the outcome, followed by validation results and remaining risks.
- Keep responses concise and use plain language unless technical detail helps the user decide or verify.

## Skills And Tools

- Use a skill when the user names it or its trigger clearly matches the task.
- Select the smallest set of skills that covers the request; avoid stacking overlapping workflows without a concrete need.
- Follow each selected skill's workflow and validation requirements.
- Treat tool names and capabilities as conditional. Use the best available equivalent when a referenced tool is unavailable.

## Editing And Code Quality

- Preserve unrelated user changes and keep edits narrowly scoped to the request.
- Prefer clear, descriptive identifiers and simple control flow.
- Keep functions and modules cohesive without imposing arbitrary size limits or speculative abstractions.
- Keep comments focused on rationale, constraints, and non-obvious behavior.
- Do not add license headers unless requested or required by an upstream-derived file.
- Use the environment's patch/editing tool for manual changes when available.
- Do not perform destructive Git operations, commit, push, or publish unless the user requests them.

## Tests And Validation

- Run the narrowest checks that meaningfully validate the changed behavior.
- Use project-native formatting, lint, type-check, test, and build commands discovered from local guidance and configuration.
- When production code changes, run an appropriate build or compile check if the project has one and the risk warrants it.
- When tests change, run the affected tests.
- Do not fix unrelated failures; document them with enough evidence for follow-up.
- Report exact validation commands, failures, skipped checks, and residual risk.

## Safety And Scope

- Distinguish review, diagnosis, and implementation requests. Do not mutate code during review-only or diagnosis-only work unless the user asks for a fix.
- Prefer reversible, local actions. Ask before irreversible actions, external publication, or meaningful scope expansion.
- Never expose secrets, credentials, private tokens, or unrelated personal data in output, logs, commits, or generated artifacts.
