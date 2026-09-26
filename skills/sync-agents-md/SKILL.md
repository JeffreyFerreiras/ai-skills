---
name: sync-agents-md
description: Audit or synchronize AGENTS.md and equivalent agent instruction files. Use for requested instruction sync, migration, or comparison across repositories and profiles.
---

# Sync AGENTS.md

## Overview

Synchronize agent-facing markdown instructions while preserving each tool's native discovery rules, local repo context, and user-owned files.

Keep a full copy of shared guidance in each requested tool's active instruction location. Do not use symlinks, thin pointer files, or instructions to read another file as a substitute for the full content.

Prefer inventory and comparison before editing. Use the user's explicit source, otherwise the canonical `ai-skills/AGENTS.md`. Timestamps indicate drift, not authority.

Apply updates only to the requested tools and repositories. Inventory other installed tools without writing to them unless cross-tool sync was requested. For skill-folder synchronization, use `sync-agent-skills` instead.

## Master Repository (`ai-skills`)

The master canonical copy for portable agent instructions (`AGENTS.md`) is maintained in the `ai-skills` git repository:
- Git URL: `https://github.com/JeffreyFerreiras/ai-skills.git`
- Master instruction file: `AGENTS.md` in the repository root.

When updating agent instructions across local repositories or profile-level tools, treat `ai-skills/AGENTS.md` as the authoritative master copy unless the user specifies a different source.

## Source Model

- The user's explicit source or target always wins.
- By default, treat the `AGENTS.md` from the master repository (`https://github.com/JeffreyFerreiras/ai-skills.git`) as the canonical source.
- When syncing a local project repository or personal profiles, synchronize guidance from `ai-skills/AGENTS.md` while preserving project-specific instructions (such as build commands, test steps, or local architecture notes) in repository-level files.
- Compare candidate files before writing. If local files contain unique project facts, preserve or merge them rather than obliterating project-specific context.
- Put the complete canonical guidance in each requested tool's active instruction file. Preserve valid tool-specific additions by merging them with the full shared content. Transform the wrapper or filename only when the tool requires it.

## Profile Sync Targets

Use these as default profile-level targets after confirming what exists locally:

- Codex: `$CODEX_HOME/AGENTS.md`, defaulting to `~/.codex/AGENTS.md`. Inspect `AGENTS.override.md` for precedence and preserve explicitly configured alternatives.
- Claude: `~/.claude/CLAUDE.md`
- Cursor: the verified active User Rule or profile rule under `~/.cursor/rules/`. This machine has `~/.cursor/rules/ai-skills.mdc` as a candidate; verify that Cursor loads it. Keep required rule frontmatter. Do not assume `~/.cursor/AGENTS.md` loads globally merely because it exists.

Codex's shared skills may live under `~/.agents/skills`; this does not change where Codex loads its global `AGENTS.md`.

Create missing parent directories when the target path is clear. Back up existing target files before overwriting them.

## Workflow

1. Locate candidate instruction files before editing. Identify the master `ai-skills` repository (`https://github.com/JeffreyFerreiras/ai-skills.git`) or target repository files.
2. Inventory each file's path, size, modified time, and apparent purpose. Use `rg --files -g "AGENTS.md" -g "CLAUDE.md" -g "copilot-instructions.md"` for fast discovery — avoid `Get-ChildItem -Recurse` which is slow.
3. Identify the source from the user's request. If none is named, treat `ai-skills/AGENTS.md` as the authoritative master copy.
4. Identify targets:
   - Repository-local targets in target projects: `AGENTS.md`, `.cursor/rules/`, `.github/copilot-instructions.md`, `CLAUDE.md`.
   - Profile-level targets for requested tools: `$CODEX_HOME/AGENTS.md` (default `~/.codex/AGENTS.md`), `~/.claude/CLAUDE.md`, and the verified Cursor instruction location. Prefer configured active files over assumed defaults.
5. Compare overlapping guidance by topic, not only by filename.
6. Decide whether to copy verbatim, merge, or transform:
   - Copy the full shared body when the target supports `AGENTS.md` semantics.
   - Merge the full shared body with valid target-specific or repo-specific guidance.
   - Transform the complete body when the target requires another format, such as Cursor rule frontmatter or VS Code prompts.
7. Before writes, state each target path and whether the operation will create, copy, merge, transform, or replace.
8. Back up existing targets with timestamped names before replacement or substantial rewrite.
9. Validate by rereading changed files, confirming each active target contains the full shared guidance, and checking markdown/frontmatter. Verify the target tool loads the destination, including any override precedence; file existence alone is not discovery evidence.

## File Discovery

Read `references/profile-files.md` when choosing target paths or when the user's request mentions VS Code, Cursor, Claude, Codex, Copilot, profile instructions, or cross-tool sync.

For repository-local work, check likely paths:

```text
AGENTS.md
AGENTS.override.md
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
- Preserve the priority `AGENTS.md` as the complete shared content in each active profile target.
- Remove chat-history details, stale task notes, and one-off implementation plans unless the user asks to keep them.
- Remove accidental duplication within one target, but keep full copies across the requested tools.
- Avoid secrets, tokens, private URLs, and credentials.
- Avoid absolute machine paths unless the file is explicitly profile-local.
- Prefer concise imperative instructions.
- Keep tool-specific additions thin and clearly labeled without shortening the shared guidance.

When conflicts appear, report the conflict and use the more specific local instruction for that target unless the user names a different source of truth.

## Write Safety

Use dry-run style reporting before cross-profile writes. Do not delete unrelated profile files. Do not overwrite a target without a backup. Do not modify global editor settings unless the request explicitly includes discovery or settings sync.
