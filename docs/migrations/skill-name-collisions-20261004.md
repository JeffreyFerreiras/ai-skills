# Skill name collision check: 2026-10-04

The canonical repository already uses `clean-code-review`. Renamed the installed
`code-review` folders in `~/.agents/skills` and `~/.claude/skills` to
`clean-code-review`. Updated frontmatter, UI metadata, invocation prompts,
fixture installation paths, and dependent review references in those roots.
Preserved installed content instead of replacing it with the master snapshot.
No old-name alias remains in either profile. Existing sessions may retain the
old skill catalog until a new session starts.

## Other names

| Name | Evidence | Action |
| --- | --- | --- |
| `loop` | Cursor has a local built-in `loop` skill. Claude lists `/loop` in its command catalog. | Migrated the personal shared and Claude profile copies to `generic-loop`. Preserved the built-ins. |
| `explain` | Generic name, but no exact built-in match found in the catalogs checked. | Optional future name: `explain-code-changes`. No rename required by this check. |
| Shared names in Claude and `.agents` | Examples include `clean-code`, `api-docs`, and `run-change-checks`. These are personal copies of shared skills. | Treat differences as profile drift. A new name alone does not fix duplicate installations. |

Avoid new personal skills named `review`, `debug`, `doctor`, `simplify`, `batch`,
or `loop`: Claude reserves these commands or bundled skills. Cursor's installed
built-ins also include `review`, `review-security`, `review-bugbot`,
`create-skill`, `create-rule`, and `migrate-to-skills`.

Sources checked: [Claude commands](https://code.claude.com/docs/en/commands),
[Cursor skills](https://cursor.com/docs/skills), and the local
`~/.cursor/skills-cursor`, `~/.cursor/skills`, `~/.claude/skills`, and
`~/.claude/commands` inventories. No other exact built-in name collision was
found among the canonical skills in these sources. This is a bounded check;
it does not cover every plugin, consumer repository, or future app release.

## Validation

Both renamed profile skills passed:

```powershell
python C:/Users/sephn/.codex/skills/.system/skill-creator/scripts/quick_validate.py C:/Users/sephn/.agents/skills/clean-code-review
python C:/Users/sephn/.codex/skills/.system/skill-creator/scripts/quick_validate.py C:/Users/sephn/.claude/skills/clean-code-review
```

A targeted `rg` scan found no old skill names or invocations in Markdown,
YAML, TOML, or Python files in either profile. Internal graph identifiers and
temporary file prefixes keep their existing names. Historical repository
migration records retain their original paths. Live app selection was not tested.

## Local migration follow-up

Renamed both personal `loop` folders to `generic-loop` and updated frontmatter,
headings, and UI prompts. Renamed the personal Cursor command
`~/.cursor/commands/code-review.prompt.md` to `clean-code-review.prompt.md`.
Updated `~/.codex/agents/code_reviewer.toml` to require `clean-code-review`.
All changes preserve each installed copy's existing workflow.

Scanned repositories directly under `C:/dev/git` and their registered worktrees
for installed roots under `.agents/skills`, `.claude/skills`, `.cursor/skills`,
`.github/skills`, and `skills`. No old-name skills or active references were
found in the six non-temporary repositories with installed roots. A temporary
publication worktree contains only leftover build files at the old paths;
it has no installed `SKILL.md` and was left untouched. The canonical repository
also has ignored Python cache files under `skills/code-review`; these are not
discoverable skills. Backups, caches, plugin-managed skills, and leftover
`.codex/skills` copies were excluded. No consumer repository files changed.

A bounded repository scan to depth three also checked the three repositories
under `C:/dev/git/public` and their registered worktrees. None has an installed
skill root. Both `generic-loop` profile copies passed the same `quick_validate.py`
command above with `generic-loop` in place of `clean-code-review`. The final
reference scan returned no old invocations; a Python check confirmed that all
four new profile folders exist, the old folders are absent, the renamed Cursor
command exists, and the Codex reviewer TOML parses and requires the new name.
`git diff --check` passed.
