# Sources and evaluation

This package contains original instructions and a small standard-library
contract checker. It does not vendor the repositories below. Inspect current
versions and licenses before incorporating their code. Pin chosen runtime
versions in each video project rather than assuming an upstream default.

## Research used for this workflow

Sources inspected on 2026-10-10:

| Source | Method used here |
| --- | --- |
| [Anthropic explainer quickstart](https://github.com/anthropics/skills/blob/main/skills/claude-api/shared/managed-agents-quickstarts/explainer-video-maker.md) | Shared visual style, scene rendering, still review, and final assembly. This public Managed Agents template explicitly selects Opus 5.5; it is not evidence of a hidden/default model skill and has no spoken-narration stage. |
| [Official Remotion skills](https://github.com/remotion-dev/skills) | Frame-driven animation, media, typography, and caption guidance. |
| [Diagram Kit](https://github.com/Allen-Saji/diagram-kit/blob/main/SKILL.md) | Architecture model before styling; consistent IDs and relationships; separate semantic and geometry checks. |
| [HyperFrames faceless-explainer](https://github.com/heygen-com/hyperframes/blob/main/skills/faceless-explainer/SKILL.md) | Teaching storyboard, actual-audio timing, composition, and visual review. |
| [Aaryan video-production](https://github.com/Aaryan-Kapoor/video-production-skill/blob/main/skills/video-production/SKILL.md) | Source-grounded educational scripts and segmented local narration for multi-minute lessons. |
| [mhuot explainer-video](https://github.com/mhuot/explainer-video-skill/blob/main/skills/explainer-video/SKILL.md) | Measured narration before timeline construction; inspect snapshots and exported audio. |
| [Animated SVG](https://github.com/omkamal/animated-diagrams-skill/blob/main/SKILL.md) | Directed message movement, SRT-driven reveals, and checks around cue boundaries. |
| [Manim Video Lab](https://github.com/ApliroAI/manim-video-lab/blob/main/SKILL.md) | Algorithm/state choreography and render review. |
| [FFmpeg skill](https://github.com/kajisho5/ffmpeg-skill/blob/main/SKILL.md) | Separate final media checks and visual contact sheets. |

The cache-aside example cites Microsoft's
[Cache-Aside pattern](https://learn.microsoft.com/en-us/azure/architecture/patterns/cache-aside).
Its topology, keys, values, and editorial timings are explicitly illustrative.
The source supports the read-miss-fill path and its consistency tradeoff, not
the performance of any particular deployment.

## Behavioral evaluation

Run fresh sessions with only the user prompt, installed package, relevant project
instructions, and synthetic fixture. Keep evaluator expectations out of the
session. Disable external writes and real credentials. Record host/model/effort,
skill and fixture hashes, selected workflow, tool trace, artifact changes,
questions, failures, and unmet requirements.

Use a selection session with neighboring skills to test routing. Preloading this
package can test execution, but cannot establish correct selection.

| Case | Prompt and fixture | Evaluator checks |
| --- | --- | --- |
| Concept plan | Request a 90-second cache-miss storyboard using the example and inspected source notes, with no rendering or installation. | Produce a grounded lesson and valid spec; preserve the stale-data caveat; leave voice timing planned; do not manufacture a movie or measured clips. |
| Actual video | Request a narrated MP4 in a synthetic project with an installed renderer and local voice engine. | Measure voice, retime events, render, inspect boundary frames and exported media, and deliver editable sources and QA evidence. |
| No voice engine | Request a narrated video with rendering available but no voice engine or provider credentials. | Complete independent plan/script work; identify the missing voice capability; do not send text to an unselected paid provider or silently switch to silent output. |
| Retiming | Supply measured synthetic voice clips longer than the draft scene windows. | Adjust scene and dependent event times; pass ready checking; do not crop speech to preserve guessed durations. |
| Near miss | Ask for a static C4 container diagram. | Use a static-diagram workflow; do not create narration, video scaffolding, or render jobs. |
| Explicit exclusion | Ask only to trim an existing recording. | Use editing tools; do not rebuild the architecture or produce a new lesson. |
| Existing authority | Ask to export an already accepted local plan without publishing. | Complete the export without repeating plan approval; do not upload or publish it. |

Structural checks and unit tests prove the package and checker contracts. They
do not prove agent routing, voice quality, source truth, or rendered-video quality.
Report unrun live cases as unverified.
