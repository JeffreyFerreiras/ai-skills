# AI Agent Skills

Canonical, portable source for personal AI-agent skills, instructions, scripts, and tool-specific metadata.

## Validate

Install the validator dependency, then run the repository doctor and tests:

```powershell
python -m pip install PyYAML
python skills/skill-doctor/scripts/skill_doctor.py .
python -m unittest discover -s tests -v
Push-Location .\skills\software-engineering-graph
python -m unittest discover -s tests -v
Pop-Location
```

The doctor validates frontmatter, folder naming, UI metadata, referenced resources, Python syntax, overlapping triggers, and optional profile drift.

## Layout

```text
AGENTS.md
skills/
  skill-name/
    SKILL.md
    agents/openai.yaml
    scripts/
    references/
    assets/
tests/
```

Only `SKILL.md` and `agents/openai.yaml` are required. Add resource folders when they directly support the skill.

## Skill Catalog

| Skill | Purpose |
| --- | --- |
| `andromeda-ssh` | Safely inspect and administer the Andromeda Ubuntu host over SSH. |
| `api-docs` | Add accurate .NET XML documentation to changed public APIs. |
| `clean-architecture-code` | Implement code with pragmatic inward-pointing boundaries. |
| `clean-architecture-review` | Review architecture boundaries and dependency direction. |
| `clean-code` | Write or refactor clear, maintainable code while preserving behavior. |
| `code-review` | Review local changes with evidence-backed, severity-ranked findings. |
| `word-documents` | Create, edit, render, and visually verify DOCX files. |
| `generate-unit-tests` | Add maintainable, risk-focused unit tests and verify them. |
| `address-pr-feedback` | Inspect and address actionable GitHub PR review threads. |
| `leetcode` | Solve and explain coding-interview and algorithm problems. |
| `loop` | Run bounded write and independent review repair cycles. |
| `recommend-model-effort` | Recommend the lowest sufficient model reasoning-effort level for a task. |
| `remove-agent-skill` | Safely remove a skill from profiles and the repository mirror. |
| `remove-slop` | Remove branch-local AI artifacts without changing behavior. |
| `skill-doctor` | Validate this repository and compare it with an installed profile. |
| `software-engineering-graph` | Orchestrate bounded design, implementation, review, and verification roles. |
| `sync-agent-skills` | Audit and synchronize skills across agent profiles. |
| `sync-agents-md` | Audit and synchronize agent instruction markdown. |
| `run-change-checks` | Select and run focused checks for current changes. |
| `vault-answer` | Answer questions strictly from evidence in the local knowledge vault. |
| `vault-build-graph` | Build an evidence-backed knowledge graph from ingested vault sources. |
| `vault-daily-review` | Create an evidence-backed daily review of vault activity. |
| `vault-find-contradictions` | Find and record dated contradictions and changed beliefs in the vault. |
| `vault-maintain` | Audit, validate, and safely maintain the local knowledge vault. |
| `vault-process-inbox` | Process vault Inbox sources and invoke semantic graph building. |
| `vault-reset` | Safely reset a knowledge vault with a recoverable backup. |

## Create A Skill

Use the installed `skill-creator` scaffolder, replace its placeholders, add the skill to the catalog, and validate the whole repository:

```powershell
$creator = Join-Path $HOME '.codex\skills\.system\skill-creator\scripts\init_skill.py'
python $creator my-skill --path .\skills --interface display_name='My Skill' --interface short_description='Describe the capability' --interface default_prompt='Use $my-skill to complete this task.'
python skills/skill-doctor/scripts/skill_doctor.py .
```

## Profile Comparison

Compare the repository with a Codex profile without changing either location:

```powershell
python skills/skill-doctor/scripts/skill_doctor.py . --profile-root (Join-Path $HOME '.codex\skills')
```

Repository content is canonical. Profile updates should use the `sync-agent-skills` workflow, begin with a dry run, and preserve backups of differing targets.
