---
name: skill-doctor
description: Validate skill structure, metadata, resources, and optional discovery/profile parity. Use when auditing, troubleshooting, or preparing skills for synchronization or publication.
---

# Skill Doctor

## Workflow

1. Run the bundled validator from the repository root:

```powershell
python skills/skill-doctor/scripts/skill_doctor.py .
```

2. Add `--profile-root <path>` when repository-to-profile parity matters.
3. Fix errors before publishing or synchronizing. Review warnings for trigger overlap, portability concerns, and profile drift.
4. Re-run the validator after changes and report the final error and warning counts.

Use `--json` for automation and `--strict` when warnings must also fail the command.

## Validation Policy

- For this repository's local schema, require `SKILL.md` frontmatter with only `name` and `description`. This is not a claim that other Codex skills cannot use optional metadata.
- Require the folder name and frontmatter name to match.
- Require `agents/openai.yaml` with matching UI metadata.
- Verify local resource links recursively, including cycles, and require the README catalog to match skill folders. Flag folders missing SKILL.md and optional discovery copies with content drift. A canonical-only checkout needs no discovery copy; a leftover discovery manifest with a missing copy is an error.
- Compile Python sources without writing bytecode.
- Flag unresolved placeholders, machine-specific paths, and highly similar trigger descriptions.
- Compare complete skill-folder content when a profile root is supplied, excluding generated cache files.

The validator is read-only. Do not automatically rewrite skills or synchronize profiles as part of diagnosis.

Structural success is not a behavioral approval. For instruction audits, review source/target scope, existing authorization, publication defaults, external dependencies, runtime requirements, and semantic trigger overlap. Use the repository's behavioral scenarios when testing these decisions; do not infer safety from zero lexical-overlap warnings.
