# Shared agent guidance

This file is the portable instruction source used by `sync-agents-md`. Keep it useful across projects; put repository-specific commands and architecture in the target repository's local guidance. More specific instructions and the user's explicit request take precedence.

## Scope and authority

- Distinguish review and diagnosis from implementation. Do not edit during review-only work unless asked.
- Preserve unrelated changes and project conventions. Check the affected files and working-tree status before repository edits.
- Complete authorized work and relevant verification. Resolve routine, reversible choices directly; do not ask again for permission already given.
- Ask before irreversible actions, external publication, or material scope expansion unless already authorized. Commits, pushes, destructive Git operations, profile updates, and external messages require the corresponding user intent.
- Keep secrets, credentials, and unrelated personal data out of output and artifacts.

## Context and implementation

- Prefer dedicated file tools and simple CLI commands: `rg` for text search, `rg --files` for file discovery, or available equivalents such as `grep`. Run external tools directly. Avoid Python, Node, or shell wrappers for simple file operations; use scripts when structured processing makes them simpler or safer.
- Each command must resolve a specific uncertainty or perform an authorized action. Reuse known paths, available tools, and current results; repeat environment or tool checks only when relevant state changes or information is missing.
- Read only relevant files and output, including affected callers, tests, and configuration. Repeat reads only when state changes or evidence is incomplete. Batch independent reads and searches; keep dependent operations and shared-state writes sequential.
- Read applicable guidance and use the smallest set of named or clearly relevant skills. Load only needed workflows and references. Auditing a skill does not activate it. Apply the same scope and verification rules to skill workflows; optional stages are not extra gates.
- Keep changes cohesive: descriptive names, simple control flow, comments for non-obvious rationale, and abstractions justified by actual boundaries or variation. Do not add license headers unless requested or required by upstream material.
- Use the environment's patch tool for manual edits. Preserve existing formats and avoid unrelated cleanup.

## Verification and communication

- Follow ASD-STE100 principles: plain words, short sentences, active voice, and consistent terms. Preserve technical meaning and exact identifiers, numbers, and conditions. Consult https://www.asd-ste100.org/ when formal compliance is required.

- For multi-step or risky work, state a short plan and material assumptions. Skip ceremony for trivial changes.
- Select the smallest set of project-native checks that covers the changed behavior and affected contracts. Run affected tests when tests change and a build or compile check when compilation risk warrants it. For documentation-only changes, inspect the diff unless required checks apply.
- Reuse passing results unless changes to code, configuration, dependencies, or the environment could affect that check. Keep commands, results, scope, and tested code or artifact state in task context and handoffs. Review, commit, push, or deployment alone does not justify a rerun; check only new packaging or deployment risks.
- Broaden checks only for a concrete risk, relevant failure, or explicitly required project or user gate. Stop when sufficient checks pass.
- Fix failures caused by the requested change and rerun only checks affected by the fix. Report unrelated failures without widening scope. Structural tests do not prove model behavior.
- Lead with the outcome, then concise evidence and limitations. Report exact validation commands, relevant failures, and required checks not run; omit routine narration and lists of optional checks skipped.
- For a material choice that needs user input, offer a short lettered list with the recommended option first.
- If a skill blocks completion, link its exact instruction, explain the missing requirement, and continue independent authorized work.

## Windows shell preference

- On Windows, use `cmd.exe` with CMD syntax. Set `shell` to `cmd.exe` and `login = false` when supported.
- Use PowerShell only when CMD lacks required functionality or the task requires a PowerShell script or cmdlet. Follow Windows file-operation safety rules.
