# Documentation audit — 2026-09-26

An implementation record for maintainers: what changed, why, and what remains unverified. [Back to the README](../README.md).

## Findings and corrections

| Finding | Correction |
| --- | --- |
| The catalog omitted `git-push`; most entries did not link to their instructions. | Generate an alphabetical, directly linked catalog from canonical names and UI summaries. |
| `git-push` lacked required UI metadata. | Add its display name, summary, and invocation prompt; leave its stage/commit/push behavior intact. |
| Long descriptions repeated tool choices and broad keyword lists. | Shorten 16 routing descriptions while retaining capability boundaries. Detailed workflows remain in the skill bodies. |
| Shared `AGENTS.md` mixed useful boundaries with repeated generic guidance. | Condense it while retaining its intentional role as the portable instruction-sync source. |
| Onboarding assumed an installed scaffolder at a particular path. | Provide a complete two-file example and keep the scaffolder optional. |
| A concurrent, user-confirmed change removed repository discovery copies and made skill sync profile-only. | Preserve that work; document canonical-only checkout and make discovery validation conditional on an existing copy or manifest. |
| The doctor only recognized unlinked catalog names; no check compared generated descriptions or documentation media paths. | Accept linked catalog rows and add catalog/link regression checks to the ordinary unit suite. |
| Behavioral guidance referred to CI although the repository workflow had been removed. | Describe repository tests accurately and explain the manual evaluation protocol. |
| PR feedback instructions asked users to select threads again even when already selected, contradicting the existing `selected-pr-feedback` behavioral scenario. They also assumed this library supplied a plugin connector. | Honor the selected threads and describe connector availability with a `gh` fallback; preserve the prohibition on unrequested GitHub writes. |
| The independent reviewer example supplied `agent_type`, which is absent from the current `collaboration.spawn_agent` schema. | Remove that unsupported argument; retain fresh context and inherited model/effort behavior. |
| Removal guidance omitted the current shared user root, and a sync reference preferred thin wrappers even for native skill folders. | Include `~/.agents/skills` in scoped inventory and prefer complete skill packages for native discovery. No installed files are changed. |
| VS Code instructions required `chat.agentSkillsLocations` for the shared root; current documentation lists that root natively and deprecates the setting. | Route current users to native discovery, label `doctor-vscode` as a legacy Local-agent settings helper, and leave its existing implementation unchanged. |

The existing rename record was read as historical evidence: `loop` is now `generic-loop`, and `code-review` is now `clean-code-review`. Historical snapshots, inventory manifests, and old commands remain history, not current onboarding instructions.

## Visual and information design

The README now introduces the purpose, shows a stable lifecycle, provides a first validation command, and links every skill. Two guides carry architecture/sync and authoring/validation detail. The graph's extensive specialist documentation stays with the graph.

The light and dark hero assets were generated with the built-in image tool. They show author → check → use → evaluate, with large headings and no changing counts or filenames. A narrow-screen preview exposed small bitmap labels, so an editable, theme-aware mobile SVG presents the same steps vertically below 600px. The README includes alt text and a text equivalent; the architecture guide includes an editable Mermaid flowchart. [Prompts](assets/skill-lifecycle-prompts.txt) preserve the visual brief. Animation was rejected because it added no information beyond the static cycle.

## Research and decisions

Sources were checked on 2026-09-26. Searches emphasized 2026-06-28 onward; older primary references are dated below rather than presented as recent releases.

| Primary source | Date or status | Decision supported |
| --- | --- | --- |
| [OpenAI: Rethinking skills and prompts for GPT-6 Astra](https://developers.openai.com/blog/rethinking-skills-and-prompts-for-gpt-6-astra) | 2026-09-11 | Short, discriminating routing descriptions; contextual guidance; remove redundant global instructions without weakening real boundaries. |
| [Anthropic: Lessons from building Claude Code](https://claude.dev/blog/lessons-from-building-claude-code-how-we-use-skills/) | 2026-06-03, outside the search window | Prefer real gotchas, reusable helper scripts, and selective supporting detail over speculative rules. |
| [Agent Skills specification](https://agentskills.io/specification) | Living specification | Distinguish the portable package from this repository's stricter metadata convention. |
| [Codex skills](https://learn.chatgpt.com/docs/build-skills), [Cursor skills](https://cursor.com/docs/skills), [Claude Code skills](https://code.claude.com/docs/en/skills) | Living product documentation | Document discovery roots and cloud limits separately; no universal runtime compatibility claim. |
| [Claude Code evaluation guidance](https://code.claude.com/docs/en/skills#evaluate-and-iterate-on-a-skill) | Living product documentation | Keep routing/execution evaluation separate from metadata and script tests. |
| [VS Code Agent Skills](https://code.visualstudio.com/docs/agent-customization/agent-skills) | Living product documentation | Remove a stale mandatory settings workaround; preserve the helper only for older Local-agent configuration. |
| [Vercel: Agent skills explained](https://vercel.com/blog/agent-skills-explained-an-faq) | 2026-01-26 | Treat skills as portable folders of reusable expertise; do not claim this repository is a plugin marketplace. |
| [GitHub diagrams](https://docs.github.com/en/get-started/writing-on-github/working-with-advanced-formatting/creating-diagrams) and [theme-aware images](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/quickstart-for-writing-on-github) | Living product documentation | Use basic Mermaid and repository-relative light/dark `<picture>` assets with a fallback. |

### X discovery limits

Searches covered X discussions of skills, `SKILL.md`, `AGENTS.md`, Codex, Claude Code, Cursor, GitHub, and portable plugins, with a preference for the last 90 days. They did not establish a useful, verifiable recent primary post. An [indexed March discussion](https://x.com/i/trending/2033693090804891779) pointed toward Thariq Shihipar's guidance on gotchas and helpers. That X page is an automated summary, not authoritative technical evidence; the decision above relies on Anthropic's published article. No X popularity claim or uncorroborated interoperability claim was imported.

## Validation boundaries

Before changes, the repository suite passed 34 tests. The doctor reported the pre-existing missing `git-push` catalog entry and metadata; after the concurrent removal it also reported missing discovery copies. Those are the concrete contradictions addressed here.

Catalog and link checks are deterministic. They cannot prove semantic correctness, successful live skill selection, external URL availability, or final hosted GitHub appearance. Live agent evaluations were not run; changed routing remains behaviorally unverified. The graph engine and orchestration bodies were preserved, so its broad engine suite was not required for these documentation/metadata edits. Its migration record contains historical Windows/SQLite failures; those were not treated as current test results or repaired here.

No commits, pushes, profile updates, or CI restoration are part of this audit. The concurrent profile-sync implementation and discovery deletions belong to separate work; documentation describes their resulting contract.

## Validation results

Commands below ran from the repository root unless stated otherwise:

| Command or check | Result |
| --- | --- |
| `python skills/skill-doctor/scripts/skill_doctor.py .` | 25 skills; zero errors and warnings. Strict mode also passed. |
| `python scripts/build-docs.py --check` | Catalog current; relative links, anchors, and image paths resolve across 49 Markdown documents. |
| `python scripts/sync-discovery.py --check` | Canonical-only checkout reported; no discovery files created. |
| `python -m unittest discover -s tests -v` | 42 tests passed on the combined working tree, including the concurrent profile-sync changes. |
| `python -m unittest discover -s tests -p test_documentation.py -v` | All six documentation regression tests passed. |
| `git -c core.safecrlf=false diff --check` | Passed. |

From `skills/software-engineering-graph`, the final hygiene check also passed (one test):

```powershell
$env:PYTHONDONTWRITEBYTECODE = '1'
$env:PYTHONNOUSERSITE = '1'
python -m unittest -v tests.test_standalone_acceptance.StandaloneAcceptanceTests.test_hygiene
```

The five Python files changed for this audit compiled without bytecode. The installed skill-creator metadata validator passed for 20 changed/new skill folders present at that checkpoint. The minimal authoring example validated in a disposable tree with no skill errors; its expected missing-README warning reflected the deliberately incomplete miniature repository. The documented sync preview flags were exercised against a disposable target; it created no target directory. A pre-existing `PureWindowsPath.is_reserved()` deprecation warning remains in the sync helper and was not repaired as unrelated work.

GitHub's Markdown API rendered the README and architecture guide successfully. Local Chromium previews used that rendered HTML with GitHub-style CSS at 1200px and 390px in light and dark modes: correct images loaded, the mobile SVG switched colors, and no horizontal page overflow appeared. Mermaid 11 rendered the basic flowchart with ten nodes. This verifies rendering inputs and local presentation, not a published repository page; no publication was performed.
