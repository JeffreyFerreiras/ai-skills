# Agent Instruction File Locations

Use these locations as search hints, not as a license to overwrite. Confirm actual files on disk before editing.

## Codex

- `~/.codex/instructions.md` for global user instructions.
- `~/.codex/skills/<skill-name>/SKILL.md` for reusable Codex skills.
- Repository-local `.codex/` files may exist when a project keeps Codex-specific guidance beside source.

## VS Code and Copilot

- Windows user settings: `%APPDATA%\Code\User`.
- Common instruction files include `.github/copilot-instructions.md` and `.github/prompts/*.prompt.md` in a repository.
- Copilot agent storage can include generated agent markdown under VS Code global storage; treat those as implementation-owned unless the user explicitly targets them.

## Cursor

- Profile root is commonly `~/.cursor`.
- User settings are commonly under `%APPDATA%\Cursor\User` on Windows.
- Repository rules commonly live under `.cursor/rules/` with `.mdc` or markdown files.

## Claude

- Profile root is commonly `~/.claude`.
- Repository-local `CLAUDE.md` is often used for project guidance.
- Claude configuration formats vary by installed product surface; inspect existing files before choosing a target.

## Repository

- Prefer repository-local `AGENTS.md` when the user wants guidance shared by multiple coding agents.
- Keep product-specific files only when a tool requires different syntax or discovery.
