# Agent Instruction File Locations

Use these locations as search hints, not as a license to overwrite. Confirm actual files on disk before editing.

## Master Repository (`ai-skills`)

- Master Git URL: `https://github.com/JeffreyFerreiras/ai-skills.git`
- Master file: `AGENTS.md` at repository root.
- All profile instruction files and repo-level common agent instructions synchronize from this master copy.

## Codex

- `$CODEX_HOME/AGENTS.md` for global instructions, defaulting to `~/.codex/AGENTS.md`. A non-empty `AGENTS.override.md` in the same directory takes precedence. Preserve an explicitly configured alternative rather than assuming `instructions.md` loads automatically.
- In this setup, shared reusable skills live under `~/.agents/skills/<skill-name>/SKILL.md`. Skill discovery is separate from the global `AGENTS.md` path.
- Repository-local `.codex/` files may exist when a project keeps Codex-specific guidance beside source.

## VS Code and Copilot

- Windows user settings: `%APPDATA%\Code\User`.
- Common instruction files include `.github/copilot-instructions.md` and `.github/prompts/*.prompt.md` in a repository.
- Copilot agent storage can include generated agent markdown under VS Code global storage; treat those as implementation-owned unless the user explicitly targets them.

## Cursor

- Profile root is commonly `~/.cursor`.
- Use the verified active Cursor User Rule or profile rule under `~/.cursor/rules/` for global guidance. On this machine, `~/.cursor/rules/ai-skills.mdc` holds a full copy with `alwaysApply: true` frontmatter; verify that Cursor loads it before treating it as active. Do not assume `~/.cursor/AGENTS.md` is globally loaded merely because it exists.
- User settings are commonly under `%APPDATA%\Cursor\User` on Windows.
- Repository rules commonly live under `.cursor/rules/` with `.mdc` or markdown files.

## Claude

- Profile root is commonly `~/.claude`.
- Use `~/.claude/CLAUDE.md` for the copied portable instruction file when no more specific existing profile instruction file is present.
- Repository-local `CLAUDE.md` is often used for project guidance.
- Claude configuration formats vary by installed product surface; inspect existing files before choosing a target.

## Profile Synchronization

- When synchronizing profile-level agent instructions, choose the priority `AGENTS.md` source, then put its full shared content in each requested tool's active profile target. Preserve tool-specific additions and any required wrapper or frontmatter; do not substitute symlinks or pointer-only files.
- Default Codex and Claude targets are `~/.codex/AGENTS.md` and `~/.claude/CLAUDE.md`; verify the active Cursor User Rule or profile rule. Honor `CODEX_HOME` and limit writes to requested tools.
- Back up existing target files before overwriting them.

## Repository

- Prefer repository-local `AGENTS.md` when the user wants guidance shared by multiple coding agents.
- Keep product-specific files only when a tool requires different syntax or discovery.
