---
name: sync-agent-skills
description: Audit, compare, and synchronize AI agent skills across installed user profiles. Use for profile skill sync, inventory, migration, backup, or consistency checks. Sync shared skills from the canonical ai-skills repository into profile roots only; do not distribute skills to individual repositories or worktrees.
---

# Sync Agent Skills

## Overview

Coordinate user-profile skill installations across local assistants while preserving each tool's native format and avoiding destructive overwrites.

Skill sync is profile-only. Do not discover consumer repositories or worktrees, copy skills into their discovery roots, or refresh repository-local mirrors. Leave existing repository copies untouched; removing them is a separate cleanup request. A request naming one profile stays limited to that profile.

Use this skill for skill folders and their discovery settings. Use `sync-agents-md` for instruction-document synchronization. Restrict writes to the requested tools and roots; an inventory does not authorize synchronization.

Prefer an inventory-first workflow. Treat `.agents/skills`, `.claude`, `.cursor`, and VS Code user-profile files as user-owned configuration unless the user explicitly asks to replace or normalize them. For this Codex-and-Cursor setup, install shared skills once in `~/.agents/skills` and synchronize them from `ai-skills/skills`; both tools discover that root locally. Keep Cursor-only skills in `~/.cursor/skills` and leave Cursor-managed built-ins alone. Do not recreate matching shared copies in Cursor's skill roots during later syncs.

## Master Repository (`ai-skills`)

The master canonical copy for all skills is the `ai-skills` git repository:
- Git URL: `https://github.com/JeffreyFerreiras/ai-skills.git`
- Master skills folder: `skills/` within the repository root.

When updating personal assistant profile roots (`~/.agents/skills`, etc.), treat `ai-skills` as the authoritative master copy. Reading this source repository is not permission to distribute skills to project repositories. Leave leftover `.codex/skills` copies untouched.

## Workflow

1. Locate the relevant roots before editing:
   - Master repository: discover or clone `https://github.com/JeffreyFerreiras/ai-skills.git` (or the local checkout of `ai-skills`).
   - Profile roots:
     - Codex user skills: `~/.agents/skills` (current Codex documentation).
     - Cursor-only user skills: `~/.cursor/skills` (current Cursor documentation).
     - Claude: `~/.claude` and skill/instruction subfolders.
     - Cursor: `~/.cursor` and Cursor user profile settings/rules folders.
     - VS Code: user profile folders such as `%APPDATA%\Code\User` on Windows.
2. Run an inventory and inspect existing formats, names, and duplicate concepts.
3. For VS Code, verify discovery through its current Agent Customizations interface. Current releases support `~/.agents/skills` directly. Use `doctor-vscode` only to inspect older Local-agent settings; it checks legacy flags and does not establish discovery in current agent harnesses. See `references/profile-locations.md` before changing settings.
4. Decide the direction of sync with the user request as the source of truth. When syncing profiles, pull latest versions from the master `ai-skills` repository.
   - For this two-tool profile setup, update shared skills in `~/.agents/skills` only. Preserve skills unique to Cursor in `~/.cursor/skills` and Cursor-managed built-ins in `~/.cursor/skills-cursor`. Cursor Cloud Agents do not receive personal skills from `~/.agents/skills`; report that limitation or use an explicitly requested Cursor Cloud sync. Do not compensate by installing project skills.
   - Master skills with `external-source.json` are installation pointers. Resolve the declared repository's latest default-branch commit (or an explicitly pinned `revision`) and install the full skill from that commit. Do not skip the skill, install the pointer itself, or treat an existing installation as current without checking upstream during an authorized sync. Ordinary skill execution does not authorize an update.
5. Transform content only when needed:
   - Shared agent skills require a folder with `SKILL.md` frontmatter.
   - Cursor commonly uses rule or instruction files.
   - VS Code/Copilot commonly uses prompt or instruction markdown files.
   - Claude commonly uses project/user instructions, commands, or skill-like markdown assets depending on the installed product surface.
6. Before writes, state the target paths and whether the operation will copy, transform, or replace files.
7. Preserve unrelated profile files, tool-specific skills, and installed-skill runtime data such as graph policies and state. Show differing installed skill paths in the dry run. Do not change consumer repositories, their branches, or application code as part of sync.
8. Validate by re-running inventory and, where applicable, checking that generated markdown/frontmatter is syntactically valid.
   - Verify profile copies against the source. For external skills, compare against the resolved upstream commit, not the pointer folder, and record repository/revision provenance. Report counts for updated profiles, resolved external skills, and any inaccessible or excluded profile roots. A failed external resolution is an incomplete sync, not a successful skip.
   - After authorized enablement changes, re-inventory the affected skill roots. Use a fresh-process discovery check when a runtime change needs verification; see `references/codex-discovery.md` for Codex.
9. When the user asks to update installed profile skills from master:
   - Identify the local `ai-skills` checkout (`https://github.com/JeffreyFerreiras/ai-skills.git`).
   - Run `sync_agent_skills.py sync-from-master --master <ai-skills-path> --target-root <profile-skills-path>`. Pass only user-profile destinations; `--target-repo` is no longer supported.
   - By default this updates existing installed skills to the latest master version. Use `--all` if newly added skills from master should also be installed.
10. When the user asks to publish profile changes back to master:
   - This is an explicitly requested contribution to the canonical source, not distribution to consumer repositories. It is not part of ordinary profile sync.
   - Use the repository specified by the user or discover the current `ai-skills` checkout (`https://github.com/JeffreyFerreiras/ai-skills.git`).
   - Mirror each changed skill folder into `<repo>\skills\<skill-name>`.
   - Inspect `git status --short --branch` before staging so unrelated user changes are visible.
   - Commit only the mirrored skill changes with a focused message when the user requests a commit.
   - Push only when the user explicitly requests publication, using the repository's configured branch.
   - Never force-push, rewrite history, or include unrelated repo changes unless the user explicitly asks.

## Helper Script

Use `scripts/sync_agent_skills.py` for repeatable local filesystem operations:

```powershell
$syncScript = Join-Path $HOME '.agents\skills\sync-agent-skills\scripts\sync_agent_skills.py'
python $syncScript inventory
```

Common operations:

```powershell
# Inventory known profile roots as JSON.
python <skill-dir>\scripts\sync_agent_skills.py inventory --json

# Use broader or narrower bounded scans when profile folders are large.
python <skill-dir>\scripts\sync_agent_skills.py inventory --max-depth 3 --max-files 100

# Inventory explicit roots.
python <skill-dir>\scripts\sync_agent_skills.py inventory --root "agents=$HOME\.agents\skills" --root "vscode=$env:APPDATA\Code\User"

# Inspect legacy VS Code Local-agent discovery settings.
python <skill-dir>\scripts\sync_agent_skills.py doctor-vscode

# Apply legacy settings only when the installed version needs them and the user requested it.
python <skill-dir>\scripts\sync_agent_skills.py doctor-vscode --apply

# Dry-run a copy from a source skill/file into a target root.
python <skill-dir>\scripts\sync_agent_skills.py sync --source "$HOME\.agents\skills\my-skill" --target-root "$HOME\.claude\skills"

# Apply the copy without a retained backup.
python <skill-dir>\scripts\sync_agent_skills.py sync --source "$HOME\.agents\skills\my-skill" --target-root "$HOME\.claude\skills" --apply --force --no-backup
```

The script does not convert formats. Use it to inventory, compare checksums, and copy a finalized artifact after deciding that a direct copy is appropriate.

Target names must be single filenames. The helper rejects overlapping trees, linked source/target entries, and linked backup paths. For `external-source.json` pointers, it fetches one concrete upstream commit, validates required resources and the skill name, and installs the full skill with a `.skill-source.json` provenance receipt. Resolution failure leaves the existing installation intact. Git metadata, nested assistant discovery roots, and Python bytecode caches are excluded from the installed snapshot.

External resolution also runs during dry runs so the preview can compare the latest content; only temporary staging files are written. The manifest requires an HTTPS `repository`, `management: "external"`, and relative `required_files`. Its optional `revision` must be a full commit hash; otherwise resolve the latest default-branch HEAD. Use `--backup-root <path-outside-skill-discovery>` only when retained backups are requested; it cannot be combined with `--no-backup`. The canonical master pointer remains unchanged.

## Profile Update from Master

To update installed profile skills from the master repository (`https://github.com/JeffreyFerreiras/ai-skills.git`):

```powershell
$masterRepo = (git rev-parse --show-toplevel).Trim() # or path to cloned ai-skills repo
$syncScript = Join-Path $masterRepo 'skills\sync-agent-skills\scripts\sync_agent_skills.py'

# Dry-run updating shared profile skills used by Codex and local Cursor
python $syncScript sync-from-master --master $masterRepo --target-root "$HOME\.agents\skills"

# Apply updates to the shared profile root without retained backups
python $syncScript sync-from-master --master $masterRepo --target-root "$HOME\.agents\skills" --apply --force --no-backup
```

## Publishing Profile Changes to Master Repository

When contributing profile changes back into the master repository (`https://github.com/JeffreyFerreiras/ai-skills.git`), keep the profile path and repository skill folder aligned:

```powershell
$repo = (git rev-parse --show-toplevel).Trim()
$skillName = "sync-agent-skills"
$source = Join-Path $HOME ".agents\skills\$skillName"
$target = Join-Path $repo "skills\$skillName"

python (Join-Path $source "scripts\sync_agent_skills.py") sync --source $source --target-root (Join-Path $repo "skills") --apply --force --no-backup
git -C $repo status --short --branch
```

Stage, commit, or push only when requested. Resolve the configured remote and target branch instead of assuming `master` or `main`.

## Format Guidance

Read `references/profile-locations.md` when choosing target paths or converting between tool-specific formats.

Keep synced content portable:

- Put long procedural knowledge in markdown.
- Avoid absolute paths unless they are intentionally machine-specific.
- Avoid secrets, tokens, private URLs, and local credentials.
- Keep tool-specific wrappers thin; duplicate the capability intent, not unnecessary metadata.

## Safety Rules

- Never use profile sync to install, update, or restore repository-local skill copies, including copies in this source repository. Do not invoke repository discovery-copy scripts as part of this workflow.
- Default to dry-runs for copy/sync operations.
- Never delete unrelated profile files.
- Preserve runtime data absent from the canonical skill snapshot.
- Do not change global VS Code, Cursor, Claude, or Codex settings unless the request explicitly includes settings sync.
- If multiple files express the same concept, report the candidates and pick the newest or most complete only when the user's intent is clear.
