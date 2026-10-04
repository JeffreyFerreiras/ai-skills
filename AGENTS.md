# Shared agent guidance

This file is the portable instruction source used by `sync-agents-md`. Keep it useful across projects; put repository-specific commands and architecture in the target repository's local guidance. More specific instructions and the user's explicit request take precedence.

## Scope and authority

- Distinguish review and diagnosis from implementation. Do not edit during review-only work unless asked.
- Preserve unrelated changes and project conventions. Check the affected files and working-tree status before repository edits.
- Complete authorized work and relevant verification. Do not ask again for permission already given; resolve routine, reversible implementation choices directly.
- Ask before irreversible actions, external publication, or material scope expansion unless already authorized. Commits, pushes, destructive Git operations, profile updates, and external messages require the corresponding user intent.
- Keep secrets, credentials, and unrelated personal data out of output and artifacts.

## Context and implementation

- Prefer dedicated file tools or simple existing CLI commands. Use `rg` for text search and `rg --files` for file discovery; use `grep` or another available equivalent when needed.
- Avoid Python, Node, or PowerShell scripts and extra shell launches for work that native tools can do directly. Use scripts when structured processing or platform operations make them simpler or safer.
- Use each command to resolve a specific uncertainty or perform an authorized action. Reuse task context and inspect only relevant files and output. Expand inspection to callers, tests, and configuration when they affect the change. Repeat inspection only when relevant state changes or prior evidence is insufficient. Batch independent reads and searches; keep dependent operations and operations that share mutable state sequential.
- Read task-relevant guidance. Use named or clearly applicable skills; choose the smallest set that covers the work. Load only the applicable workflow and supporting references. Auditing a skill does not activate its workflow.
- Skill workflows must reuse completed steps and valid check results. Optional stages must not expand the task or add redundant checks. Preserve explicitly required project and user gates.
- Tool names are conditional. Use an available equivalent without inventing capabilities or weakening authorization boundaries.
- Keep changes cohesive: descriptive names, simple control flow, comments for non-obvious rationale, and abstractions justified by actual boundaries or variation. Do not add license headers unless requested or required by upstream material.
- Use the environment's patch tool for manual edits. Preserve existing formats and avoid unrelated cleanup.

## Verification and communication

- Use ASD-STE100 Simplified Technical English principles for explanations, instructions, and documentation: plain words, short sentences, active voice, and consistent technical terms. Preserve exact identifiers, numbers, conditions, and necessary technical meaning. For formal STE compliance, check the official writing rules and controlled dictionary at https://www.asd-ste100.org/.

- For multi-step or risky work, state a short plan and material assumptions. Skip ceremony for trivial changes.
- Select the smallest set of project-native checks that covers the changed behavior and affected contracts. Run affected tests when tests change and a build or compile check when compilation risk warrants it. For documentation-only changes, inspect the diff unless required checks apply.
- Reuse passing check results while the relevant code, configuration, dependencies, and test environment remain unchanged. Preserve commands, results, scope, and the tested code or artifact state in task context and handoffs. A move to review, commit, push, or deployment alone does not justify a rerun. Use focused checks for new packaging or deployment risks.
- Broaden checks only for a concrete risk, relevant failure, or explicitly required project or user gate. Stop when sufficient checks pass.
- Fix failures caused by the requested change. Rerun failed checks and any checks whose results the fix invalidates. Report unrelated failures instead of widening scope. Do not treat structural tests as proof of model behavior.
- Lead with the outcome, then evidence and limitations. Report exact validation commands, failures, and skipped checks.
- When a material choice is needed, offer a short lettered list with the recommended option first. Do not turn routine decisions or authorized actions into approval gates.
- If a skill blocks completion, link its exact instruction, explain the missing requirement, and continue independent authorized work.

## Windows shell preference

- On Windows, use native CMD (`cmd.exe`) for command execution. When the command tool supports it, select CMD as `shell` and set `login = false`.
- Use PowerShell only when the required functionality is unavailable in CMD or the task requires a PowerShell script or cmdlet. Do not choose PowerShell merely for convenience.
- Use CMD syntax for CMD commands. Run external command-line tools directly without extra shell wrappers. Follow all applicable Windows file-operation safety rules.
