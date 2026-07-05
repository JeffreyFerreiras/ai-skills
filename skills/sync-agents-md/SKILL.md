---
name: sync-agents-md
description: Audit, compare, create, and synchronize AGENTS.md-style instruction markdown across repositories and profile-level AI agent tools. Use when the user asks to sync AGENTS.md, agent instructions, Copilot/VS Code prompts, Cursor rules, Claude instructions, Codex instructions, or wants an inventory, migration, backup, or consistency check for agent-facing markdown guidance.
---

# Sync AGENTS.md

## Overview

Synchronize agent-facing markdown instructions while preserving each tool's native discovery rules and avoiding destructive overwrites.

Treat instruction files as user-owned configuration. Prefer inventory and comparison before editing, and keep canonical content separate from tool-specific wrappers when formats differ.

## Canonical Source

The profile-level `C:\Users\sephn\AGENTS.md` is the single source of truth. Sync always flows **from the profile outward to repos** — never from a repo back to the profile unless explicitly requested. Repo-level AGENTS.md files are downstream copies.

## Workflow

1. Locate candidate instruction files before editing, latest edited wins.
2. Inventory each file's path, size, modified time, and apparent purpose. Use `rg --files -g "AGENTS.md" -g "CLAUDE.md" -g "copilot-instructions.md"` for fast discovery — avoid `Get-ChildItem -Recurse` which is slow.
3. Identify the canonical source from the user's request. Default to `C:\Users\sephn\AGENTS.md` unless told otherwise.
4. Compare overlapping guidance by topic, not only by filename.
5. Decide whether to copy verbatim, merge, or transform:
   - Copy when the target also supports `AGENTS.md` semantics.
   - Merge when the target already has user-specific local rules.
   - Transform when the target uses another format such as Cursor rules, VS Code prompts, or Claude user instructions.
6. Before writes, state each target path and whether the operation will create, append, merge, or replace.
7. Back up existing targets with timestamped names before replacement or substantial rewrite.
8. Validate by rereading changed files and checking that markdown/frontmatter remains syntactically sane.

## File Discovery

Read `references/profile-files.md` when choosing target paths or when the user's request mentions VS Code, Cursor, Claude, Codex, Copilot, profile instructions, or cross-tool sync.

For repository-local work, check likely paths:

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

Use `rg --files -g "AGENTS.md" -g "CLAUDE.md" -g "copilot-instructions.md" -g "*.prompt.md" -g "*.mdc"` from the relevant root. This is significantly faster than `Get-ChildItem -Recurse` on Windows.

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
