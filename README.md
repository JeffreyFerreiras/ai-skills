# AI Agent Setups

Portable repository for AI-agent skills, instructions, prompts, scripts, and tool-specific setup wrappers that can be replicated across machines and AI tools.

## Layout

```text
skills/
  example-skill/
    SKILL.md
    agents/
      openai.yaml
    scripts/
    references/
    assets/
```

Each reusable skill should be a self-contained folder under `skills/`. Keep each setup asset focused:

- `SKILL.md` is required.
- `agents/openai.yaml` is recommended for UI metadata.
- Add `scripts/`, `references/`, or `assets/` only when they directly support the skill.
- Avoid extra documentation inside individual skill folders unless an agent needs it to use the skill.

## Creating A Codex-Format Skill

For Codex-format `SKILL.md` assets, use lowercase hyphenated names:


```powershell
python C:\Users\sephn\.codex\skills\.system\skill-creator\scripts\init_skill.py my-skill --path .\skills
```

Then edit `skills/my-skill/SKILL.md` and validate it:

```powershell
python C:\Users\sephn\.codex\skills\.system\skill-creator\scripts\quick_validate.py .\skills\my-skill
```
