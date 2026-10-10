# Rendering and media checks

Choose a backend from the existing project and the lesson's visual needs. Resolve
tools and versions from its package files and command help. Keep one rendering
engine for a simple lesson; use an independently rendered Manim insert only when
its mathematics or algorithm choreography justifies stitching.

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

Local Kokoro is one option, not a required global installation. Use an available
voice engine or the user's selected provider. Record its version, voice, speed,
pronunciation overrides, generated clip paths, and measured durations. Resolve
credentials through the configured client without printing them.

For narrated output, generate SRT or VTT from actual alignment and inspect cue
order, line length, reading speed, and end times. If only scene-level timing is
available, create phrase-level cues conservatively and disclose that alignment
precision. Burn subtitles into the picture only when requested or required by
the destination. Keep text within the final format's safe area.

## Final media inspection

Use FFprobe's JSON output to inspect the actual exported file:

```bash
ffprobe -v error -show_streams -show_format -of json outputs/explainer.mp4
ffmpeg -hide_banner -i outputs/explainer.mp4 -af volumedetect -f null -
```

Compare dimensions, fps, codecs, streams, and duration with the brief and spec.
A narrated export requires an audio stream. Check picture/audio start and end
times within 0.1 seconds of the intended cut, apart from a declared end hold or
encoder padding. Check measured peaks for clipping, then review intelligibility
and voice/music balance by listening when the host supports it. If listening is
unavailable, say so; waveform or transcript analysis does not replace it.

For platform delivery, use that platform's current audio and container guidance.
A common web/YouTube target is H.264 with AAC and fast-start MP4. Two-pass loudness
normalization may target -14 LUFS and -1 dBTP when appropriate for that destination;
measure first and retain measured values for pass two. Keep the video stream
unchanged when only fixing audio or packaging. The optional
[FFmpeg skill](https://github.com/kajisho5/ffmpeg-skill/blob/main/SKILL.md) supplies
repeatable probe, caption, loudness, export, and contact-sheet tools.

Write a QA report with observed values, inspected frame times, source/claim
review, executed commands, and remaining defects. Distinguish media validation
from architectural correctness and live-agent behavior.
