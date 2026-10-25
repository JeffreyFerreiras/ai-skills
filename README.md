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

## Vertical system design videos

Use [`system-design-video`](skills/system-design-video/SKILL.md) to teach one system mechanism in a phone-friendly **9:16 video**, normally **1080x1920 at 30 fps**. About **90 seconds** is a planning estimate; scene lengths follow the measured narration. A request for only a script or storyboard stops at that artifact.

```mermaid
flowchart TD
    A["Sources + teaching question"] --> B["Grounded lesson + architecture<br/>Actors, success path, failure or tradeoff"]
    B --> J["Lock and review the spoken hook<br/>Accurate promise, first frame, immediate payoff"]
    J --> K{"Independent script approval<br/>Technical truth + narrative flow"}
    K -->|"Repair within 3 total rounds"| B
    K -->|"Blocked or unresolved at cap"| X
    K -->|"Current revision approved"| C["Record narration and measure timing<br/>Align events and subtitles to speech"]
    C --> D["Narrated vertical sample<br/>Show a real state change and transition"]
    D --> E{"Independent asset reviews<br/>Voice, visual, logical + media/story"}
    E -->|"Repair within the shared 3-round cap"| D
    E -->|"Unresolved at cap"| X["Report findings and limits"]
    E -->|"Applicable checks pass"| F["Full timeline + local render<br/>Requests follow real directed relationships"]
    F --> G["Inspect encoded video<br/>Readability, playback, audio and sync"]
    G --> H["Deliver one continuous video<br/>Subtitles + editable source + QA evidence"]
    classDef plan fill:#e0f2fe,stroke:#0369a1,color:#0c4a6e;
    classDef review fill:#fef3c7,stroke:#b45309,color:#78350f;
    classDef output fill:#dcfce7,stroke:#15803d,color:#14532d;
    class A,B,J,C plan;
    class K,E,G,X review;
    class H output;
```

**Read the diagram from top to bottom:** the sourced lesson determines the narration; measured speech determines animation timing. The sample tests whether the explanation works before the full render. Blue boxes prepare the lesson, amber boxes show review or evidence limits, and green marks delivery.

Each scene must show a meaningful change, such as a request arriving before its response leaves. Reviews record what was actually inspected: sampled frames and automated media checks cannot establish continuous watching or listening. Delivery does not imply viewer acceptance or permission to publish. See the [production review](skills/system-design-video/references/production-review.md) and [audio and delivery](skills/system-design-video/references/audio-and-delivery.md) guides.

Start with a production document and an opening that explains the problem, relevance, solution, and learning points in lesson order. Default new local narration to [Kokoro/OpenVINO on explicit CPU](skills/system-design-video/references/kokoro-narration.md), with a configured voice, isolated runtime and bounded worker. Keep MOSS-TTS Local Transformer v1.5 and Qwen3-TTS as explicit alternatives; preserve approved recordings. A model choice does not approve a particular voice or authorize regenerating episodes. Use authentic chalkboard scenes with crisp drawn diagrams, varied causal action, upright labels, and high word highlighting. Review every asset and assign a dedicated visual reviewer to every scene within the shared three-round cap. Chat handoffs covered by the brief include a separate full-length 1080p review copy alongside the continuous HQ master. See [local narration and its measured limits](skills/system-design-video/references/local-narration.md).

Before final visual timing, a dedicated voice reviewer checks cadence, sentence pauses, identity, energy, and joins. Short samples prefer one continuous take; pause-only edits preserve PCM speech and refresh downstream cues. New cinematic directions start as standalone motion samples with internal activity and camera motion, then await feedback before integration. Instrumented checks do not count as perceptual listening or style approval.

Lock and review a captivating spoken question, punchy phrase, or defensible claim before full narration. It must appear in a narrated video's first seconds and lead into the explanation; an on-screen question or optional animated cover supports it and cannot replace it. Preserve unaffected approved audio when adding missing speech, and verify the actual encoded opening audio. The sourced relevance/curiosity/payoff rubric treats roughly 3-6 seconds as flexible advertising guidance, not a voice-speed target or retention guarantee. Pair chalk explanations with the accepted hand-drawn layered-depth transition direction, and use a separate fresh-context logical reviewer to check object roles and complete causal paths through temporal evidence. Keep all reviewers within the existing asset budget. User-facing copy uses no em dashes. See [hook development](skills/system-design-video/references/hook-development.md).

The review loop is built into the video skill: one bound Writer, validation of the exact artifact revision, fresh independent Reviewers, explicit finding dispositions, and recoverable state. Independent technical and narrative script approval comes before TTS or rendering; voice consistency and every-scene visual/logical gates precede final delivery approval. Three total rounds per deliverable is a ceiling, with early stop when clear. Material changes invalidate affected approvals. See [the built-in video loop](skills/system-design-video/references/video-review-loop.md).

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
| [orchestrator-mode](skills/orchestrator-mode/SKILL.md) | Choose a helper model with model=<provider/model> (defaults to Muse) while the primary only thinks |
| [notebooklm-cinematic-video](skills/notebooklm-cinematic-video/SKILL.md) | Prepare and activate longer NotebookLM lessons |
| [notebooklm-short-video](skills/notebooklm-short-video/SKILL.md) | Prepare and activate digestible NotebookLM shorts |
| [opencode-go-compare](skills/opencode-go-compare/SKILL.md) | Compare OpenCode Go models on speed, cost, and benchmarks |
| [opencode-muse-spark](skills/opencode-muse-spark/SKILL.md) | Delegate scoped tasks to Muse Spark via OpenCode |
| [recommend-model-effort](skills/recommend-model-effort/SKILL.md) | Choose the right reasoning effort for a task |
| [remove-agent-skill](skills/remove-agent-skill/SKILL.md) | Remove a skill from all agent tools |
| [remove-slop](skills/remove-slop/SKILL.md) | Clean AI artifacts without behavior changes |
| [review-panel](skills/review-panel/SKILL.md) | Run a seven-role independent review panel |
| [root-cause-analysis](skills/root-cause-analysis/SKILL.md) | Trace failures to evidence-backed causes |
| [run-change-checks](skills/run-change-checks/SKILL.md) | Run focused checks for changes |
| [skill-doctor](skills/skill-doctor/SKILL.md) | Validate and audit a skills repository |
| [software-engineering-graph](skills/software-engineering-graph/SKILL.md) | Orchestrate rigorous application delivery |
| [sync-agent-skills](skills/sync-agent-skills/SKILL.md) | Sync user-profile skills without repository copies |
| [sync-agents-md](skills/sync-agents-md/SKILL.md) | Audit and reconcile agent instructions |
| [system-design-video](skills/system-design-video/SKILL.md) | Create narrated vertical system design videos |
| [theme-factory](skills/theme-factory/SKILL.md) | Apply consistent colors and fonts to artifacts |
| [to-tickets](skills/to-tickets/SKILL.md) | Draft verifiable tickets with explicit blockers |
| [unslop](skills/unslop/SKILL.md) | Remove AI phrasing and filler from writing |
| [word-documents](skills/word-documents/SKILL.md) | Create and verify .docx files |
<!-- skill-catalog:end -->

Looking for `loop` or `code-review`? They are now `generic-loop` and `clean-code-review` to avoid built-in command collisions. See the [migration record](docs/migrations/skill-name-collisions-20260926.md).

## How it works

For optional local Codex task advice, see the [Laya hook setup](docs/laya-hook.md).
The hook keeps the current primary model and uses a resident Windows service to
recommend Luna, Sol, or Astra for delegated work.

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
