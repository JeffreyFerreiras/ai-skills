# Skill name collision migration

Renamed two canonical skills to avoid shadowing harness-provided commands:

| Previous name | New name |
| --- | --- |
| `loop` | `generic-loop` |
| `code-review` | `clean-code-review` |

Checked the [Cursor built-in skill catalog](https://cursor.com/docs/skills#built-in-cursor-skills) and [Claude Code command catalog](https://code.claude.com/docs/en/commands) on 2026-09-26. These were the matching names in this repository's 20-skill catalog.

Updated folder names, frontmatter, display names, invocation prompts, behavioral fixture installation, graph reviewer references, and the committed Cursor discovery copy. Preserved all 52 files from the original folders. Historical migration manifests retain their original source paths; internal review-graph identifiers and temporary-file prefixes are not skill commands and remain unchanged. The latest master branch removed the validation workflow; this migration preserves that removal.

## Installed copies

This change updates the canonical repository. Separately installed profiles and consumer repositories still need migration. Compare each installed copy with the canonical source, back up differing copies outside skill discovery roots, install the new names, and retire the old personal copies. Do not leave old-name aliases: they recreate the collisions. Update invocations and dependent reviewer profiles together.

## Validation before updating the base

Commands run from the repository root with Python 3.13.2 and `PYTHONDONTWRITEBYTECODE=1`:

```powershell
./scripts/setup-discovery.ps1
python skills/skill-doctor/scripts/skill_doctor.py .
python C:/Users/sephn/.codex/skills/.system/skill-creator/scripts/quick_validate.py skills/generic-loop
python C:/Users/sephn/.codex/skills/.system/skill-creator/scripts/quick_validate.py skills/clean-code-review
python -m unittest discover -s tests -q
python -m unittest discover -s skills/clean-code-review/tests -q
git -c core.safecrlf=false diff --check
```

Results: all 20 skills validated without errors or warnings; both individual validators passed; 27 repository tests and 19 review-skill tests passed, including the C# fixture build check. Diff whitespace checks passed. The setup helper created the documented local Windows discovery junction after the first doctor run found a plain-file symlink checkout.

From `skills/software-engineering-graph`, with `PYTHONDONTWRITEBYTECODE=1` and `PYTHONNOUSERSITE=1`:

```powershell
python -m unittest -q tests.test_contracts tests.test_planner tests.test_validator tests.test_state tests.test_cli tests.test_graph_hardening tests.test_reviewer_delegation
```

Result: 190 tests ran, with 134 errors and one failure. Run initialization raises `OverflowError: Python int too large to convert to SQLite INTEGER` in unchanged `graph_engine/state.py`. The concurrent-initialization assertion also fails because neither initialization succeeds. A standalone temporary-directory/SQLite probe reproduced the overflow: this Windows volume's `st_dev` exceeds SQLite's signed 64-bit integer range. No graph engine code was changed to address this unrelated environment compatibility issue.

The initial graph invocation without `PYTHONNOUSERSITE=1` failed to import all seven modules because a user-installed package named `tests` shadowed the graph tests. Disabling user-site packages for the command resolved that import issue.

Final graph hygiene is checked separately with:

```powershell
python -m unittest -q tests.test_standalone_acceptance.StandaloneAcceptanceTests.test_hygiene
```

Live-agent behavioral evaluations were not run for this mechanical rename. Deterministic tests do not establish live harness selection behavior.
