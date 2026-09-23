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

## Suggestions And Choices

- Whenever you make a suggestion or recommendation to the user, including an action, next step, or decision, present a short lettered list (`A.`, `B.`, and so on) with distinct choices. Put your recommended choice first and label it as recommended.
- Make each choice clear enough that the user can select it by replying with only its letter. Include a practical alternative, such as deferring or skipping, when there is no other useful option.
- If the user has already authorized an action, carry it out without asking them to choose again. Do not add a choice list to factual answers, status updates, or completed work that makes no suggestion.

## Skills And Tools

- Use a skill when the user names it or its trigger clearly matches the task.
- Select the smallest set of skills that covers the request; avoid stacking overlapping workflows without a concrete need.
- Follow each selected skill's workflow and validation requirements.
- Treat tool names and capabilities as conditional. Use the best available equivalent when a referenced tool is unavailable.
- Explicit user intent and existing authorization take precedence over skill defaults, within higher-priority constraints. Perform authorized work before requesting missing approval; do not repeat an approval already given.
- If a skill blocks progress, identify and link its exact instruction, explain the missing requirement, and continue independent authorized work. Reading a skill for an audit does not activate its operational commands.

## Editing And Code Quality

- Preserve unrelated user changes and keep edits narrowly scoped to the request.
- Prefer clear, descriptive identifiers and simple control flow.
- Keep functions and modules cohesive without imposing arbitrary size limits or speculative abstractions.
- Keep comments focused on rationale, constraints, and non-obvious behavior.
- Do not add license headers unless requested or required by an upstream-derived file.
- Use the environment's patch/editing tool for manual changes when available.

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
