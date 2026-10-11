# AI Skills

Reusable workflows for coding agents, maintained in one canonical repository and loaded when the task needs them.

My working library of engineering, review, orchestration, and maintenance skills—plain instructions, practical helpers, and checks that keep them honest.

[Quick start](#quick-start) · [Skill catalog](#skill-catalog) · [How it works](docs/architecture.md) · [Authoring & validation](docs/authoring.md)

<picture>
  <source media="(max-width: 600px)" srcset="docs/assets/skill-lifecycle-mobile.svg">
  <source media="(prefers-color-scheme: dark)" srcset="docs/assets/skill-lifecycle-dark.png">
  <source media="(prefers-color-scheme: light)" srcset="docs/assets/skill-lifecycle-light.png">
  <img src="docs/assets/skill-lifecycle-light.png" alt="Skill lifecycle: author canonical skills, check structure and resources, discover and load on demand, then evaluate actual behavior and refine." width="100%">
</picture>

**Author → check → use → evaluate.** A skill is a folder containing a `SKILL.md` entrypoint and, when useful, scripts, references, and assets. Structural checks validate the package; observed agent behavior tells us whether the workflow works.

## Quick start

From a checkout of this repository, use Python **3.12+**:

```powershell
python -m pip install PyYAML
python skills/skill-doctor/scripts/skill_doctor.py .
```

Then pick a skill below and read its entrypoint. To use it in an agent, install the complete folder into that host's supported skill location. In Codex, for example: `Use $clean-code to simplify this function while preserving behavior.` See [discovery and profile setup](docs/architecture.md#discovery-and-profiles) for supported roots and a read-only sync preview.

To create or edit a skill, work under `skills/`, supply its UI metadata, then refresh the catalog and check it:

```powershell
python scripts/build-docs.py
python skills/skill-doctor/scripts/skill_doctor.py .
python scripts/sync-discovery.py --check
python -m unittest discover -s tests -v
```

The [authoring guide](docs/authoring.md) includes a minimal complete example. Discovery checking is read-only: a canonical-only checkout needs no repository copy, and the command says so. It checks parity if an optional Cursor copy exists.

## Long-form horizontal video production

Use [`produce-long-form-video`](skills/produce-long-form-video/SKILL.md) for a complete technical explanation in **16:9**, normally **1920x1080**, with native-paced local narration. Chapters build on one continuing example; the explanation determines duration. A request for only an outline, script, or storyboard stops at that artifact.

```mermaid
flowchart TD
    A["Primary sources + learning outcome"] --> B["Chapter questions + storyboard<br/>Mechanism, consequence, tradeoff, recap"]
    B --> C["Narrated sample + concept-specific scenes<br/>Actors, comparisons, timelines and state changes"]
    C --> D{"Independent technical/editorial<br/>and scene diva review"}
    D -->|"Repair within the shared 3-round cap"| C
    D -->|"Unresolved at cap"| X["Report findings and limits"]
    D -->|"Sample gates pass"| E["Full animatic at native pace<br/>Check explanations, transitions and geometry"]
    E --> F["Freeze exact sources<br/>Artwork + script + WAV + cues + code"]
    F --> G["Render checked, resumable batches<br/>Draw each frame once; reuse verified work"]
    G --> H["Review encoded master<br/>Visuals, playback, loudness and audio sync"]
    H --> I["Deliver continuous full-quality video<br/>Captions + production record + recovery ZIP"]
    classDef plan fill:#e0f2fe,stroke:#0369a1,color:#0c4a6e;
    classDef review fill:#fef3c7,stroke:#b45309,color:#78350f;
    classDef output fill:#dcfce7,stroke:#15803d,color:#14532d;
    class A,B,C,F plan;
    class D,E,H,X review;
    class I output;
```

**Read the diagram from top to bottom:** prove the teaching approach in a narrated sample, review the full timeline, then freeze the exact inputs for a reproducible render. Blue boxes prepare the material, amber boxes show review or evidence limits, and green marks delivery. Each deliverable has at most three shared review rounds; changing reviewers does not reset that limit.

**Visual variety must explain something.** An actor view follows a request, a comparison exposes a choice, and a timeline explains event order or expiry. The scene diva checks composition, readability and purposeful motion; technical/editorial review checks correctness and the explanation. Native narration remains unchanged during visual repairs, while cached caption layouts and reusable lossless batches reduce repeated work.

The final encode still needs playback and media checks. Frame sampling and a successful full decode do not prove continuous watching or listening; record missing evidence explicitly. Delivery, viewer acceptance and publication remain separate states. See the [scene vocabulary](skills/produce-long-form-video/references/scene-vocabulary.md), [production gates](skills/produce-long-form-video/references/production.md), and [chapter validation report](docs/validation/produce-long-form-video-20261011.md).

## Skill catalog

Every canonical skill, alphabetically. Names come from `SKILL.md`; summaries come from `agents/openai.yaml`. Regenerate with `python scripts/build-docs.py`; the test suite rejects stale output.

<!-- skill-catalog:start -->
| Skill | Purpose |
| --- | --- |
| [a11y-debugging](skills/a11y-debugging/SKILL.md) | Check accessibility with Chrome DevTools |
| [address-pr-feedback](skills/address-pr-feedback/SKILL.md) | Address actionable GitHub feedback |
| [andromeda-ssh](skills/andromeda-ssh/SKILL.md) | Connect to and safely administer Andromeda |
| [api-docs](skills/api-docs/SKILL.md) | Document public C# APIs accurately |
| [bug-hunter-review](skills/bug-hunter-review/SKILL.md) | Find concrete bugs introduced by a change |
| [chrome-devtools](skills/chrome-devtools/SKILL.md) | Inspect and test web pages in Chrome |
| [clean-architecture-code](skills/clean-architecture-code/SKILL.md) | Write code with clean boundaries |
| [clean-architecture-review](skills/clean-architecture-review/SKILL.md) | Review code for architecture drift |
| [clean-code](skills/clean-code/SKILL.md) | Write and refactor pragmatic clean code |
| [clean-code-review](skills/clean-code-review/SKILL.md) | Review defects and design opportunities |
| [create-pull-request](skills/create-pull-request/SKILL.md) | Create GitHub pull requests |
| [discernment-nudge](skills/discernment-nudge/SKILL.md) | Prompt reflection on key facts and assumptions |
| [doc-coauthoring](skills/doc-coauthoring/SKILL.md) | Co-author specs, proposals, and decision documents |
| [domain-modeling](skills/domain-modeling/SKILL.md) | Sharpen domain terms, glossaries, and decisions |
| [explain](skills/explain/SKILL.md) | Explain code changes with a sequence diagram |
| [frontend-design](skills/frontend-design/SKILL.md) | Design distinctive, responsive web interfaces |
| [generate-unit-tests](skills/generate-unit-tests/SKILL.md) | Add focused, maintainable unit tests |
| [generic-loop](skills/generic-loop/SKILL.md) | Run bounded write and independent review cycles |
| [git-push](skills/git-push/SKILL.md) | Commit and push all pending changes on request |
| [grilling](skills/grilling/SKILL.md) | Stress-test decisions through a focused interview |
| [independent-reviewer](skills/independent-reviewer/SKILL.md) | Request a fresh, read-only subagent review |
| [leetcode](skills/leetcode/SKILL.md) | Solve and review coding interview problems |
| [little-helper](skills/little-helper/SKILL.md) | Delegate one tightly scoped job to a subagent |
| [mcp-builder](skills/mcp-builder/SKILL.md) | Build MCP servers for APIs and services |
| [muse-only-orchestrator](skills/muse-only-orchestrator/SKILL.md) | Orchestrate with little Muse helpers while the primary only thinks |
| [opencode-muse-spark](skills/opencode-muse-spark/SKILL.md) | Delegate scoped tasks to Muse Spark via OpenCode |
| [produce-long-form-video](skills/produce-long-form-video/SKILL.md) | Create long-form 16:9 technical explainer videos |
| [recommend-model-effort](skills/recommend-model-effort/SKILL.md) | Choose the right reasoning effort for a task |
| [remove-agent-skill](skills/remove-agent-skill/SKILL.md) | Remove a skill from all agent tools |
| [remove-slop](skills/remove-slop/SKILL.md) | Clean AI artifacts without behavior changes |
| [root-cause-analysis](skills/root-cause-analysis/SKILL.md) | Trace failures to evidence-backed causes |
| [run-change-checks](skills/run-change-checks/SKILL.md) | Run focused checks for changes |
| [skill-doctor](skills/skill-doctor/SKILL.md) | Validate and audit a skills repository |
| [software-engineering-graph](skills/software-engineering-graph/SKILL.md) | Orchestrate rigorous application delivery |
| [sync-agent-skills](skills/sync-agent-skills/SKILL.md) | Sync user-profile skills without repository copies |
| [sync-agents-md](skills/sync-agents-md/SKILL.md) | Audit and reconcile agent instructions |
| [theme-factory](skills/theme-factory/SKILL.md) | Apply consistent colors and fonts to artifacts |
| [to-tickets](skills/to-tickets/SKILL.md) | Draft verifiable tickets with explicit blockers |
| [unslop](skills/unslop/SKILL.md) | Remove AI phrasing and filler from writing |
| [word-documents](skills/word-documents/SKILL.md) | Create and verify .docx files |
<!-- skill-catalog:end -->

Looking for `loop` or `code-review`? They are now `generic-loop` and `clean-code-review` to avoid built-in command collisions. See the [migration record](docs/migrations/skill-name-collisions-20260926.md).

## How it works

| Piece | Responsibility |
| --- | --- |
| [`skills/`](skills/) | Canonical skill folders. Edit these rather than installed copies. |
| `SKILL.md` | Short selection metadata followed by workflow instructions. |
| `agents/openai.yaml` | Display name, catalog summary, invocation prompt, and optional icons. Required here; not part of the portable minimum. |
| `scripts/`, `references/`, `assets/` inside a skill | Deterministic helpers and task-specific supporting material. |
| [`AGENTS.md`](AGENTS.md) | Shared profile guidance, also used as the default source by `sync-agents-md`. |
| Installed profiles | Copies used by each agent host; updated through the profile sync workflow. |
| Optional `.cursor/skills/` | Explicitly generated project discovery copy. This checkout does not ship one. |

The portable file format is shared by Codex, Cursor, Claude Code, and other Agent Skills clients. **Discovery paths, tools, and execution capabilities remain host-specific.** Some skills require PowerShell, GitHub access, document tools, or subagent support. Read the [compatibility notes](docs/architecture.md#discovery-and-profiles) before installing.

**Software Engineering Graph** goes beyond an ordinary instruction skill: it bundles a coordination engine, role profiles, schemas, and tests. A supervisor runs an approved plan with bounded design and delivery loops. The engine records state and evidence; the host dispatches agents. Start with its [overview](skills/software-engineering-graph/README.md), not an unplanned graph run.

## Create a skill

1. Choose one repeatable capability and a name that does not shadow host commands.
2. Write a brief description that distinguishes when the skill applies. Keep tool recipes in the body.
3. Keep the entrypoint lean. Link deeper references at the point they become relevant; add scripts for repeatable operations and gotchas from actual failures.
4. Add `agents/openai.yaml`, regenerate the catalog, and run the checks. Evaluate routing or workflow changes with realistic requests.

Use an installed `skill-creator` if available, or follow the [complete local example](docs/authoring.md#create-or-edit-a-skill). No scaffolder is required.

## Quality gates

| Check | What it establishes |
| --- | --- |
| [Skill doctor](skills/skill-doctor/SKILL.md) | Skill schema, metadata, resources, Python syntax, catalog membership, and optional copy parity. |
| [Documentation check](docs/authoring.md#prevent-catalog-drift) | Generated catalog freshness; local documentation links, anchors, and image paths. |
| [Discovery check](docs/architecture.md#optional-project-discovery-copy) | Optional Cursor copy parity, or an explicit report that no copy is configured. |
| Repository unit tests | Repository helper behavior and documentation drift regression checks. |
| [Behavioral scenarios](tests/behavioral/README.md) | Live selection, authorization, actions, and results in isolated workspaces. Separate from unit tests. |

Passing deterministic checks does **not** establish semantic review quality or reliable model behavior. Nested skill suites and live evaluations have separate procedures in the [validation guide](docs/authoring.md#validation).

## Go deeper

- [Architecture, discovery, and synchronization](docs/architecture.md): source ownership, host boundaries, and how copies reach profiles.
- [Authoring, validation, and maintenance](docs/authoring.md): examples, quality gates, and visual maintenance.
- [Documentation audit and sources](docs/documentation-audit.md): findings, decisions, research dates, and validation limits.
- [Graph migration](docs/migrations/software-engineering-graph-20260906.md): preserved source history and inventory.
