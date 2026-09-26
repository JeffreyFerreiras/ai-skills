# Authoring, validation, and maintenance

For contributors creating or changing skills and their documentation. [Back to the README](../README.md).

## Create or edit a skill

Edit the canonical folder under `skills/`. Inspect neighboring workflows and host command names before adding another one. In particular, keep the renamed `generic-loop` and `clean-code-review`; old-name aliases would recreate collisions.

A new skill needs two files. This complete minimal example illustrates a narrow documentation task; adapt it to an actual capability rather than adding unused scaffolding.

`skills/release-notes/SKILL.md`:

```markdown
---
name: release-notes
description: Draft release notes from a selected commit range. Use when asked to summarize a release for users.
---

# Release Notes

Read the selected range and the project's release-note conventions. Group user-visible changes,
explain migration steps when supported by the diff, and distinguish fixes from new capabilities.
Link evidence for each item. Return a draft; publish only when explicitly requested.
```

`skills/release-notes/agents/openai.yaml`:

```yaml
interface:
  display_name: "Release Notes"
  short_description: "Draft release notes from selected changes"
  default_prompt: "Use $release-notes to draft release notes from the selected commit range."
```

Names must match their folder, use lowercase hyphen-case, and fit within 64 characters. This repository accepts only `name` and `description` in skill frontmatter. Its UI metadata requires a display name, short description, and default prompt containing `$skill-name`. These are local conventions, not universal Agent Skills requirements.

An installed `skill-creator` can scaffold those files, but it is not bundled here and its installation path varies. The two-file example works without it. Add resource folders only when needed; if an intentionally empty directory must be tracked, use `.gitkeep`.

## Spend context where it helps

Metadata is the selection surface. Say what the skill does and when it applies in the fewest words that still distinguish it from nearby workflows. Put tool choices, exhaustive examples, and procedural detail in the body or references. A review skill should not compete with an implementation skill simply because both mention code.

OpenAI's September 11 guidance recommends brief, discriminating descriptions and contextual instructions rather than mandatory reading itineraries. Here that means trimming routing metadata and shared guidance while preserving real execution contracts. [Rethinking skills and prompts for GPT-6 Astra](https://developers.openai.com/blog/rethinking-skills-and-prompts-for-gpt-6-astra).

Use progressive disclosure deliberately:

| Layer | Include | Avoid |
| --- | --- | --- |
| Description | Capability, trigger, meaningful exclusion | Tool recipes and long synonym lists |
| `SKILL.md` | Outcome, actual constraints, workflow selection | A mandatory tour of every reference |
| `references/` | Detail needed for a particular branch | Duplicated instructions or copied manuals |
| `scripts/` | Repeatable operations with known inputs and outputs | A wrapper that adds no reliable behavior |
| `assets/` | Templates, schemas, fixtures, reusable visual material | Unused examples or live run state |

Record gotchas when an observed failure would change the next run. Reuse tested helpers when deterministic behavior matters. Anthropic's [lessons from its own skills](https://claude.dev/blog/lessons-from-building-claude-code-how-we-use-skills/) support this approach. Avoid importing product-specific hooks or state paths into an otherwise portable workflow.

Do not split a small entrypoint merely to reach a line-count target. Conversely, moving a complicated protocol into references needs behavioral evidence that its gates remain discoverable; reducing file size alone is not success.

## Validation

Run from the repository root with Python 3.12+ and PyYAML installed:

```powershell
python scripts/build-docs.py
python scripts/build-docs.py --check
python skills/skill-doctor/scripts/skill_doctor.py .
python scripts/sync-discovery.py --check
python -m unittest discover -s tests -v
```

| Gate | Scope and limits |
| --- | --- |
| Documentation builder | Regenerates only the marked README catalog. Check mode is read-only. Checks local Markdown links/anchors and HTML image paths in root Markdown and all Markdown under `docs/`, `skills/`, and `tests/`. It does not fetch external URLs or render images. |
| Skill doctor | Parses metadata, checks required UI fields and bundled references, compiles Python sources without bytecode, checks catalog membership and optional mirror/profile parity. Trigger overlap is lexical evidence, not semantic evaluation. |
| Repository tests | Exercise helper behavior, sync safeguards, fixtures, and documentation drift. They use local disposable data; they do not run live agent scenarios. |
| Discovery check | Checks an optional Cursor copy; absent copy and absent manifest is valid. It never installs or restores skills in check mode. |
| Profile comparison | `skill-doctor --profile-root` compares a supplied installation; it does not modify it or verify live discovery. |

`skill-doctor --json` emits machine-readable results; `--strict` makes warnings fail too. Repository tests do not recursively collect the suites bundled inside individual skills.

When editing the C# review harness, run its own tests:

```powershell
python -m unittest discover -s skills/clean-code-review/tests -v
```

For graph changes, use the scoped commands and runtime requirements in the [graph validation section](../skills/software-engineering-graph/README.md#contributor-validation). Its standard-library engine supports Python 3.9+, while repository tooling requires 3.12+ for filesystem APIs. Graph documentation-only changes do not require its engine suite. Keep the graph's separate hygiene check last when its contributor instructions require it.

Run `git diff --check` before handing off. Do not commit or publish merely because checks pass.

## Behavioral evaluation

Use [the evaluation protocol](../tests/behavioral/README.md) and [scenario catalog](../tests/behavioral/cases.json) when routing, instruction scope, or authorization changes. Include positive requests, near misses, and explicit exclusions; keep expected outcomes hidden from the agent being evaluated.

Record the model and effort, exact skill revision, fixture, selected skill, tool trace, questions, changed artifacts, and unmet requirements. Assess execution, not only the final prose. A correct filename or severity from the C# grader does not establish a correct explanation of a defect.

An evaluation must isolate external writes and use synthetic credentials/data. The scenario catalog is a protocol and fixture specification, not a one-command live-model runner. If no live evaluation ran, report **unverified behavior**, even when every deterministic test passed. Anthropic's [skill evaluation guidance](https://code.claude.com/docs/en/skills#evaluate-and-iterate-on-a-skill) describes complementary trigger and execution evaluation.

## Prevent catalog drift

The README catalog is generated from each canonical folder's frontmatter name and UI `short_description`. No second description list or category mapping is maintained. Alphabetical order makes names predictable to find.

After adding, removing, or renaming a skill, or changing its UI summary, run `python scripts/build-docs.py`. Check mode and the ordinary unit suite compare the full generated block; a forgotten skill or stale summary fails. Missing entrypoints or metadata fail generation instead of silently disappearing. Handwritten README content outside the markers is preserved.

## Maintain visuals

The [light](assets/skill-lifecycle-light.png) and [dark](assets/skill-lifecycle-dark.png) hero images show the stable author/check/use/evaluate cycle. They contain no counts, product names, or commands. Their [generation prompts](assets/skill-lifecycle-prompts.txt) are preserved for reproducibility. Below 600px, the picture uses an editable [mobile SVG](assets/skill-lifecycle-mobile.svg) with readable stacked labels and internal light/dark styling; keep its four labels aligned with the hero. The editable architecture diagram lives in [architecture.md](architecture.md).

GitHub supports [Mermaid diagrams](https://docs.github.com/en/get-started/writing-on-github/working-with-advanced-formatting/creating-diagrams) and [theme-aware picture elements](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/quickstart-for-writing-on-github). Keep diagrams to basic flowchart syntax, use repository-relative images, supply alt text, and repeat essential information in prose. Check both color schemes and a narrow viewport when changing assets. A local preview is useful, but it is not proof of the final hosted GitHub renderer.

A static cycle is sufficient here; animation would add motion and maintenance without explaining an additional concept. Avoid introducing a documentation framework or a new CI workflow just to render this small library.
