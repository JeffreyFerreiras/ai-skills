---
name: remove-agent-skill
description: Remove a named AI agent skill from profile-level Codex, Claude, Cursor, VS Code/Copilot-facing locations, and the ai-skills repository mirror. Use when the user asks to delete, uninstall, remove, deprecate, or purge a skill across agent tools, especially when they name tools such as Codex, Claude, Cursor, VS Code, Copilot, or the ai-skills repo.
---

# Remove Agent Skill

## Overview

Remove one skill concept across local agent tool surfaces without deleting unrelated files that merely contain matching words. Prefer inventory, explicit target paths, scoped deletion, and focused repository commits.

## Workflow

1. Normalize the requested skill name to lowercase hyphen-case, but also search for likely aliases in prompts and command files.
2. Inspect repo status before changing `C:\dev\GitHub\ai-skills`; unrelated changes may already exist.
3. Inventory active tool locations:
   - Codex: `C:\Users\sephn\.codex\skills\<skill-name>`
   - Claude: `C:\Users\sephn\.claude\skills\<skill-name>`
   - Cursor: `C:\Users\sephn\.cursor\skills\<skill-name>`, `C:\Users\sephn\.cursor\skills-cursor\<skill-name>`, and `C:\Users\sephn\.cursor\commands\<skill-name>.prompt.md`
   - VS Code/Copilot: `C:\Users\sephn\AppData\Roaming\Code\User`; look for active prompt, instruction, or agent files, not history or transcript artifacts.
   - Repo mirror: `C:\dev\GitHub\ai-skills\skills\<skill-name>`
4. Search before deletion with `rg --files` and a content search. Exclude caches, logs, archived sessions, history, extension installs, and transcript stores unless the user explicitly asks to purge history.
5. Before writes, state each target path and whether it will be removed. If matches include ambiguous files, explain which are active targets and which are ignored.
6. Remove only confirmed active skill artifacts. Verify resolved paths stay inside the intended roots before recursive deletion.
7. Validate with another active-path search for the skill name and aliases.
8. In the repo, stage only the removed skill folder or directly related prompt files. Do not stage unrelated existing edits.
9. Commit the repo removal with a focused message such as `Remove <skill-name> skill`.
10. Push to `origin master` when the repo mirror is part of the requested sync.

## Search Guidance

Use path search first:

```powershell
rg --files `
  'C:\Users\sephn\.codex\skills' `
  'C:\Users\sephn\.claude\skills' `
  'C:\Users\sephn\.cursor\skills' `
  'C:\Users\sephn\.cursor\skills-cursor' `
  'C:\Users\sephn\.cursor\commands' `
  'C:\dev\GitHub\ai-skills\skills' |
  rg -i '(^|[\\/])<skill-name>([\\/.]|$)|<alias>'
```

Then run a scoped content search against active roots. Treat matches in caches, logs, archived sessions, VS Code `History`, VS Code `workspaceStorage` transcripts, and installed extensions as non-active by default.

## Deletion Safety

When using PowerShell, resolve and check each target before deletion:

```powershell
$resolved = (Resolve-Path -LiteralPath $target).Path
if (-not $resolved.StartsWith($allowedRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "Refusing to remove outside allowed root: $resolved"
}
Remove-Item -LiteralPath $resolved -Recurse -Force
```

Use one shell end to end for removal. Do not build deletion commands by piping paths into another shell.

## Validation

Confirm:

- active tool roots no longer contain the skill folder, prompt, command, or instruction file;
- scoped content search no longer finds active references;
- `git diff --cached` contains only the intended repo mirror deletion;
- unrelated worktree changes remain unstaged and are reported separately.
