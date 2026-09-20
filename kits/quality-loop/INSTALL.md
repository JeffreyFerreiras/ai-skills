# Install — Quality Loop Kit

## What you get
Six agent skills + this profile (`AGENTS.md`) + playbooks so Cursor / Claude / Codex follow one review→repair loop.

## Cursor
1. Copy the six skill folders into `.cursor/skills/` (project) or your user skills directory.
2. Copy or merge `kits/quality-loop/AGENTS.md` guidance into your project `AGENTS.md` (or keep this file and point agents at it).
3. Reload the agent session.
4. Ask: “Run the quality loop on this branch” or name a skill (`code-review`, `loop`, …).

## Claude Code
1. Copy each skill folder into `.claude/skills/` or `~/.claude/skills/`.
2. Keep `AGENTS.md` at the repo root or merge the Quality Loop section.
3. Start a new session and invoke by skill name / job description.

## Codex / other Agent Skills clients
Same folder drop: one directory per skill containing `SKILL.md`.

## Verify
Run `skill-doctor` from this repo (or `python skills/skill-doctor/scripts/skill_doctor.py .` when present) and confirm the six Kit skills are listed.

## License
See repository `LICENSE` / Kit terms. Personal and commercial use of the Kit in your own work is allowed. Do not resell or republish the skill source as your own pack.
