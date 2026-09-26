# Shared agent guidance

This file is the portable instruction source used by `sync-agents-md`. Keep it useful across projects; put repository-specific commands and architecture in the target repository's local guidance. More specific instructions and the user's explicit request take precedence.

## Scope and authority

- Distinguish review and diagnosis from implementation. Do not edit during review-only work unless asked.
- Preserve unrelated changes and project conventions. Check the affected files and working-tree status before repository edits.
- Complete authorized work and relevant verification. Do not ask again for permission already given; resolve routine, reversible implementation choices directly.
- Ask before irreversible actions, external publication, or material scope expansion unless already authorized. Commits, pushes, destructive Git operations, profile updates, and external messages require the corresponding user intent.
- Keep secrets, credentials, and unrelated personal data out of output and artifacts.

## Context and implementation

- Use `rg` or a more precise available search tool. Expand inspection to callers, tests, and configuration when they affect the change.
- Read guidance relevant to the task. Use named or clearly applicable skills; choose the smallest set that covers the work. Auditing a skill does not activate its workflow.
- Tool names are conditional. Use an available equivalent without inventing capabilities or weakening authorization boundaries.
- Keep changes cohesive: descriptive names, simple control flow, comments for non-obvious rationale, and abstractions justified by actual boundaries or variation. Do not add license headers unless requested or required by upstream material.
- Use the environment's patch tool for manual edits. Preserve existing formats and avoid unrelated cleanup.

## Verification and communication

- For multi-step or risky work, state a short plan and material assumptions. Skip ceremony for trivial changes.
- Run checks that establish the changed behavior. Use project-native commands; run affected tests when tests change and an appropriate build/compile check when production changes warrant it.
- Fix failures caused by the requested change. Report unrelated failures instead of widening scope. Do not treat structural tests as proof of model behavior.
- Lead with the outcome, then evidence and limitations. Report exact validation commands, failures, and skipped checks.
- When a material choice is needed, offer a short lettered list with the recommended option first. Do not turn routine decisions or authorized actions into approval gates.
- If a skill blocks completion, link its exact instruction, explain the missing requirement, and continue independent authorized work.
