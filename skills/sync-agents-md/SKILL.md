---
name: sync-agents-md
description: Audit, compare, create, and synchronize profile-level AGENTS.md-style instruction markdown across AI agent tools. Use when the user asks to sync AGENTS.md, agent instructions, Copilot/VS Code prompts, Cursor rules, Claude instructions, Codex instructions, or wants an inventory, migration, backup, or consistency check for profile-level agent-facing markdown guidance.
---

# Sync AGENTS.md

## Overview

Synchronize profile-level agent-facing markdown instructions while preserving each tool's native discovery rules and avoiding destructive overwrites.

Treat instruction files as user-owned configuration. Prefer inventory and comparison before editing, and keep canonical content separate from tool-specific wrappers when formats differ.

Default to profile-level scope for every task. Only inspect or modify repository-local instruction files when the user explicitly asks for repo-local sync or names a repository path as the target.

When profile-level `AGENTS.md` is created, updated, or selected as canonical, also mirror it to the AI skills repository and push that repository when the user asks to sync or publish the change.

## Workflow

1. Locate candidate instruction files before editing.
2. Inventory each file's path, size, modified time, and apparent purpose.
3. Identify the canonical source from the user's request. If unclear, prefer the newest complete profile-level instruction file or ask before replacing anything.
4. Compare overlapping guidance by topic, not only by filename.
5. Decide whether to copy verbatim, merge, or transform:
   - Copy when the target also supports `AGENTS.md` semantics.
   - Merge when the target already has user-specific local rules.
   - Transform when the target uses another format such as Cursor rules, VS Code prompts, or Claude user instructions.
6. Before writes, state each target path and whether the operation will create, append, merge, or replace.
7. Back up existing targets with timestamped names before replacement or substantial rewrite.
8. If profile-level `AGENTS.md` changed or is the requested source, copy it to the AI skills repository root as `AGENTS.md`, commit the repository change, and push when requested.
9. Validate by rereading changed files and checking that markdown/frontmatter remains syntactically sane.

## File Discovery

Always read `references/profile-files.md` when choosing target paths. Treat profile-level Codex, Claude, Cursor, VS Code, and Copilot locations as the primary search space.

For the AI skills repository mirror, prefer the local checkout at `C:\dev\GitHub\ai-skills` when present. If it is not present, locate a git repository named `ai-skills` before creating a new checkout or target path.

For repository-local work only when explicitly requested, check likely paths:

```text
AGENTS.md
.agents/AGENTS.md
.codex/instructions.md
.github/copilot-instructions.md
.github/prompts/*.prompt.md
.cursor/rules/*.mdc
.cursor/rules/*.md
CLAUDE.md
```

Use `rg --files -g "AGENTS.md" -g "CLAUDE.md" -g "copilot-instructions.md" -g "*.prompt.md" -g "*.mdc"` from the relevant root when possible.

## Merge Rules

Keep shared guidance portable:

- Preserve concrete project facts, commands, conventions, and safety rules.
- Remove chat-history details, stale task notes, and one-off implementation plans unless the user asks to keep them.
- Avoid secrets, tokens, private URLs, and credentials.
- Avoid absolute machine paths unless the file is explicitly profile-local.
- Prefer concise imperative instructions.
- Keep tool-specific sections thin and clearly labeled.

When conflicts appear, report the conflict and use the more specific local instruction for that target unless the user names a different source of truth.

## Write Safety

Use dry-run style reporting before cross-profile writes. Do not delete unrelated profile files. Do not overwrite a target without a backup. Do not modify global editor settings unless the request explicitly includes discovery or settings sync.
