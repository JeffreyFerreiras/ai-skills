# Profile Locations and Format Notes

Use this as a starting map, then verify paths on the local machine. Agent products change their profile layouts over time, and users may override defaults.

## Master Skills Repository

- Git URL: `https://github.com/JeffreyFerreiras/ai-skills.git`
- Canonical skills tree: `skills/` within the repository root.
- Installed user-profile skills sync against this master repository. Consumer repositories and worktrees are not sync destinations.
- A master skill containing `external-source.json` points to the actual upstream skill repository. During installation or sync, resolve its latest default-branch commit (or declared pinned revision) and install the full skill with revision provenance. Compare that installation with the resolved source, not the pointer. Do not copy a pointer over an engine or skip it as already current.

## Repository-Local Copies Are Outside Sync Scope

Do not scan local checkouts or worktrees for skill installations. Do not install, refresh, or restore repository-local `.agents/skills`, `.cursor/skills`, `.claude/skills`, `.github/skills`, `.codex/skills`, or `skills/` copies. Existing copies remain untouched unless the user separately requests cleanup. The helper no longer supports `--target-repo`; use explicit user-profile `--target-root` paths.

The canonical `ai-skills/skills` tree remains the source. An explicitly requested contribution back to that source is separate from distributing skills to consumer repositories.

## Codex and Cursor Profile Skills

- Codex user skill root: `~/.agents/skills` in the current [Codex skill documentation](https://learn.chatgpt.com/docs/build-skills).
- Cursor also discovers `~/.agents/skills` locally, according to the current [Cursor skill documentation](https://prod.cursor.com/docs/skills). For this setup, install shared skills there once and synchronize them from the canonical `ai-skills/skills` tree.
- Keep Cursor-only personal skills in `~/.cursor/skills`. Avoid duplicate shared copies in that root and preserve Cursor-managed built-ins in `~/.cursor/skills-cursor`.
- Skill shape: one folder per skill with required `SKILL.md` frontmatter containing `name` and `description`.
- Optional resources: `scripts/`, `references/`, `assets/`, and `agents/openai.yaml`.
- The older Codex changelog mentions `~/.codex/skills`; current Codex skill documentation lists `~/.agents/skills` for user skills. Do not move this setup's Codex skills to `.codex/skills` based on the older changelog.

## Claude

- Start with `~/.claude`.
- Look for markdown instructions, command folders, or skill-like folders already present before creating new structure.
- Preserve Claude-specific file names and metadata instead of forcing Codex `SKILL.md` layout.

## Cursor

- Start with `~/.cursor`.
- If `~/.cursor/skills-cursor` exists, treat it as Cursor-managed. A matching name can still have different behavior, as with Cursor's built-in `loop`; compare content before removing a duplicate and preserve distinct built-ins.
- Project skills for Cursor Desktop and Cursor Cloud live under workspace `.cursor/skills` (also `.agents/skills` and `.claude/skills`). Cursor can sync personal skills from `~/.cursor/skills` to Cloud Agents when its Sync Skills setting is enabled. It does not sync personal `~/.agents/skills` to Cloud Agents.
- Do not create or refresh repository copies to provide cloud availability. Cloud enablement is separate from profile file synchronization and requires its own user request.
- Cursor's third-party import setting can suppress `~/.agents/skills` in the IDE. Keep that import enabled for this setup; changing it also affects other third-party imports.
- Also inspect Cursor application user data when relevant, especially on Windows under `%APPDATA%\Cursor\User`.
- Cursor rule files may use `.mdc` or markdown-like instruction formats. Preserve existing frontmatter conventions.

## VS Code and GitHub Copilot

- On Windows, start with `%APPDATA%\Code\User`.
- Also check profile-specific folders if the user uses VS Code profiles.
- Prompt and instruction files are commonly markdown-based. Preserve file suffixes already used in the profile, such as `.prompt.md` or `.instructions.md`.
- Current [VS Code documentation](https://code.visualstudio.com/docs/agent-customization/agent-skills) lists `.github/skills`, `.claude/skills`, and `.agents/skills` for projects, and `~/.copilot/skills`, `~/.claude/skills`, and `~/.agents/skills` for personal skills (checked 2026-09-26).
- `chat.agentSkillsLocations` is deprecated and only used by the Local agent. Do not require it for current native discovery. Use the Agent Customizations interface to inspect loaded skills and the selected harness.
- The bundled `doctor-vscode` is a legacy settings helper. It checks or writes `chat.useAgentSkills`, `github.copilot.chat.skillTool.enabled`, and `chat.agentSkillsLocations`; missing flags are not proof of a current discovery failure. Use its apply mode only for a version that still needs those settings and an explicit settings-update request.

## Sync Strategy

Choose one of three strategies per target:

1. Direct copy: only when both tools can consume the same file or folder structure.
2. Thin wrapper: create a native target file that points to or summarizes the shared source.
3. Native conversion: rewrite the content into the target tool's expected markdown/frontmatter style.

For native Agent Skills locations in Cursor, Claude Code, or VS Code, copy the complete `SKILL.md` folder and its resources. Use a wrapper or native conversion only for an explicitly requested non-skill surface, such as a rule or prompt file, whose format requires it; do not replace a working skill package with a summary.
