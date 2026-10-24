# Rendering and media checks

Choose a backend from the existing project and the lesson's visual needs. Resolve
tools and versions from its package files and command help. Keep one rendering
engine for a simple lesson; use an independently rendered Manim insert only when
its mathematics or algorithm choreography justifies stitching.
Require current independent technical/narrative script and storyboard approval
before rendering even the representative sample. Use the
[built-in loop](video-review-loop.md) for scene, sample and encoded-output gates;
material scene/timing/encode changes invalidate affected evidence and need fresh
review before dependent delivery approval.

## Visual direction and assets

Honor the chosen style and supplied references. Inspect actual reference pixels;
record identity, material, lighting, texture, palette, and composition matches
and mismatches. Do not infer an aesthetic match from a filename or prompt.
Apply the [scene director handoff](scene-direction.md) to each new episode.
When no style is selected, prefer an authentic matte charcoal chalkboard for
technical teaching views with
soft overlapping eraser marks and restrained grain, plus crisp drawn technical
diagrams and semantic colors. Compare supplied board references as actual pixels;
uniform speckles, scratch masks, or a flat green gradient are not substitutes.
Keep foreground edges and text clean rather than coating every object in noise.
This drawn chalk direction permits tasteful dimensional or 3D forms when they
help explain the lesson. Honor explicit alternatives. Compose for 9:16; do not shrink
a wide architecture diagram until its labels become unreadable.

Give each mechanism a useful composition: topology for boundaries, lanes for
ordering, or a comparison for divergent states. Vary scenes with the concept;
do not reuse one diorama or the same generic block for unrelated roles. Use fresh
episode-specific artwork, retaining shared code and design tokens where useful.
Keep stable IDs and colors across
views. Preserve a useful layout during focused polish. Shallow depth and shadows
must not obscure arrow direction, node identity, or level labels. Background
texture stays subordinate and stable through motion and compression. Keep detailed
labels upright and front-facing during teaching holds. Purposeful whole-scene
transitions complement internal causal actions; camera motion alone is insufficient.

## Cinematic motion experiments

Evaluate contextual scenes, metaphors, spatial changes, before/after comparisons
and illustrative cutaways for their teaching value. Require the director's
purposeful fresh transition scene, without a quota of scene types. Retain useful
brand elements and technical continuity; a static diagram with decorative camera
motion is not a new explanation. Inspect the references actually supplied.

Consider richly hand-drawn, layered-depth transitions alongside readable chalkboard
explanations when the brief leaves transition style open. An original illustrated
prototype earned style approval through user feedback. That selects a treatment,
not acceptance of a future voiced integration. Earlier material-only polished
3D prototypes are not the current style baseline; tasteful 3D remains available
when explicitly selected or useful to the lesson. Keep labels crisp and causal
actions grounded regardless of method.

Separate original detailed background, transparent foreground subject, and near
occluder assets so planes move independently. Preserve full artwork prompts,
alpha, and provenance; avoid baked text and render readable UI/labels in code.
Use overscan and deterministic transforms to avoid exposed edges. A stable
audience anchor, differently moving depth layers, and independently timed UI or
sketch actions provide authored 2.5D motion. Do not call this true 3D or imply
character locomotion from parallax. Inspect actual reference motion before
choosing a method; observed movement does not reveal its original production tool.

Require scene-internal motion: status lights change, screens show activity,
request particles travel, or layered objects transform with the narrated event.
Add camera motion only where it helps direct understanding. Panning or zooming a static
image alone does not establish internally animated cinematic motion. Inspect
actual-paced temporal evidence spanning the scene and cutback, plus the decoded
result, not only attractive stills. Record whether inspection sampled frames or
used continuous playback. A silent style sample cannot certify caption placement,
narration sync, or the logical flow of a new voiced edit.

Use a focused motion sample when useful within the authorized production and
existing internal review gates. Obtain external feedback before integration only
when the user has reserved that decision. Record each sample's requested scope, authorization,
render/review status, temporal evidence, feedback, and integration decision.
Authorized or in-progress samples remain unapproved until feedback exists; do
not treat generation as validation. Preserve accepted audio, explanations, and
source checkpoints while the experiment remains separate. Use the existing
three-round asset budget for internal fixes, without inventing a passed review.

## Visual assets and caption layout

Create topic-specific art where the explanation needs it; reuse authorized assets
and design components only where the brief permits reuse. Check actual decoded pixel dimensions,
crop dimensions, and the largest projected footprint during motion. A requested
4K generation or upscaled bitmap is not evidence of native detail. Preserve aspect
ratio and inspect crop sharpness in the final encode; repair soft sources instead
of hiding them with blur or glow. Record asset/font provenance and licenses.

Inspect travel corridors, departure/docking, occlusion, glows, and transition
states, not just endpoint bounding boxes. Keep captions in screen space outside
camera transforms and platform overlays. Measure text with the selected font;
inspect wrapping and contrast at actual delivery size. For the default narrated
chalk lesson, use high, screen-space word-highlighted subtitles clear of platform
overlays. Honor explicit caption overrides. Derive highlighting from checked cues,
not guessed reading speed; leave pauses and end holds without a falsely active
word. Keep stable
teaching holds between meaningful causal changes.

## Checkpoint and resume

Before long renders, record hashes for scene code, spec, script, artwork, original
WAVs/cues, fonts, and dependency locks, plus runtime versions and exact commands.
Render bounded scene/frame ranges when the backend supports them. Record completed
units and their input/output hashes; use temporary outputs until a unit succeeds.
Resume only units with matching inputs and render settings. Changed shared inputs
invalidate every affected unit and downstream assembly; do not mix old and new
revisions. If the backend cannot resume safely, restart the affected render.

Keep frame timing deterministic, preserve complete speech, and inspect joins for
gaps, repeated frames, caption boundaries, and audio continuity. An interrupted
render is not a complete export. For recovery, retain the source bundle and
verify that paths resolve, dependencies are recorded, and a representative unit
can be reproduced. Document assets or runtime dependencies that cannot be bundled.

## Remotion

Use React/SVG for persistent component diagrams and request/message traces.
Derive animation state from `useCurrentFrame()` and the spec's event times.
Convert seconds to frames in one shared function. Keep fonts, measurements,
colors, node positions, and arrow routes consistent. Do not use wall-clock
timers, unseeded randomness, or CSS animation for frame-dependent state.

The [official agent skills](https://github.com/remotion-dev/skills) document the
current APIs. If installing skills is authorized, their documented command is
`npx skills add remotion-dev/skills`. They are an external dependency, not bundled
in this package. A workflow can still use installed Remotion without them.

[Diagram Kit](https://github.com/Allen-Saji/diagram-kit/blob/main/SKILL.md) is an
optional library for architecture semantics and diagram elements. It requires
its full checkout; the skill file alone does not supply the components. Resolve
its actual checkout instead of copying the upstream author's machine paths.
Geometry checks cover only the elements and frames they inspect. Check moving
states and architecture semantics separately.

Preview in the available Studio/player. For a requested MP4, use the project's
documented render command, commonly `npx remotion render`. This skill's explicit
video/export request takes precedence over another skill's preview-only default.
Observe [Remotion's current framework license](https://www.remotion.dev/docs/license/pricing)
when choosing it for company work; an open skill repository does not determine
the renderer's licensing terms.

## HyperFrames

Use HTML/SVG/GSAP for technical diagram scenes. The
[upstream faceless-explainer](https://github.com/heygen-com/hyperframes/blob/main/skills/faceless-explainer/SKILL.md)
provides storyboards, narration alignment, composition rules, captions, and
review. When available, use its relevant routing, animation, and creative
references. Its CLI and companion skills must be installed separately.

Register a paused, seekable timeline. Use finite repeats and deterministic
inputs. Give audio elements explicit IDs and use the current runtime's media
placement rules. Verify the exported audio stream, not just browser playback.
Use the installed CLI's lint, check, snapshot, and render commands. Check help
for actual flags before running them because the upstream interface changes.

Use local narration when available and appropriate. HeyGen-backed voices or
music can require credentials, uploads, and credits. Do not select them merely
because a skill mentions them; respect the user's provider and spending scope.

## Manim

Use Manim Community for algorithms, probability, equations, and precise spatial
transformations. Reuse the project's pinned version and renderer. ManimGL and
Manim Community are different runtimes; do not mix their APIs. Optional
[Manim Video Lab](https://github.com/ApliroAI/manim-video-lab/blob/main/SKILL.md)
documents a pinned Docker route and technical choreography.

Generate the voice track before coding timed actions. Drive waits and reveals
from measured word/phrase cues or scene clip durations. Preserve actual renderer
time when scheduling later beats. A preview render is useful for motion checks;
check the final resolution before delivery. An optional
[word-timed workflow](https://github.com/PaulLemaistre/explainer-video/blob/main/skills/explainer-video/SKILL.md)
uses Speechify by default, which requires an account and sends text to that
service. Reuse the timing method with the selected voice engine rather than
assuming that API is authorized.

## Audio and subtitles

Use [local narration](local-narration.md) for the Kokoro CPU default, explicit
MOSS/Qwen alternatives, synthetic-reference limits and configurable storage.
No engine is a required global installation. Record voice/version,
prosody settings, pronunciation overrides, generated clip paths, and measured
durations. Resolve
credentials through the configured client without printing them.
Keep original WAVs and cues unchanged; derive mixes and normalized audio as new
files. Adjust the timeline to fit native speech, including the final partial
frame. Do not trim the last word or speed up audio to meet a guessed frame count.

For narrated output, generate SRT or VTT from actual alignment and inspect cue
order, line length, reading speed, and end times. If only scene-level timing is
available, create phrase-level cues conservatively and disclose that alignment
precision. Use burned word highlighting for the default narrated chalk treatment
and retain a separate subtitle track. For other briefs, burn subtitles only when
requested or required by the destination. Keep text within the final safe area.

## Final media inspection

Use FFprobe's JSON output to inspect the actual exported file:

```bash
ffprobe -v error -show_streams -show_format -of json outputs/explainer.mp4
ffmpeg -v error -xerror -i outputs/explainer.mp4 -f null -
```

Compare dimensions, fps, codecs, streams, and duration with the brief and spec.
A narrated export requires an audio stream. Check picture/audio start and end
times within 0.1 seconds of the intended cut, apart from a declared end hold or
encoder padding. Inspect a full decode and actual encoded frames around scene
boundaries and dense motion. Map additional delivery streams explicitly when
checking files with more than one audio/video track. Probe/decode success does
not establish readable labels, motion quality, or listening.

For platform delivery, use that platform's current audio and container guidance.
A common web/YouTube target is H.264 with AAC and fast-start MP4. For narrated
exports, follow [audio and delivery](audio-and-delivery.md) to measure integrated
LUFS and true peak on each final encode, repair from measured inputs, and verify
sync and actual listening. Sample-peak checks alone do not establish AAC headroom
or comfortable speech volume. Keep the video stream unchanged for audio-only
repairs. The optional
[FFmpeg skill](https://github.com/kajisho5/ffmpeg-skill/blob/main/SKILL.md) supplies
repeatable probe, caption, loudness, export, and contact-sheet tools.

Write a QA report with observed values, inspected frame times, source/claim
review, executed commands, and remaining defects. Distinguish media validation
from architectural correctness and live-agent behavior.
