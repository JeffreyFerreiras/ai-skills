# Production contract

Use `video-spec.json` when scenes share an architecture or have narration-driven
events. This is a renderer-independent input contract. The bundled checker
validates references, declared evidence, causal order, scene coverage, and audio
fit. Human review must still establish source truth, teaching quality, visual
readability, and whether the renderer obeys the spec.

## Fields

| Object | Required fields and meaning |
| --- | --- |
| Root | `schema_version: 2`, `question`, `audience`, `profile`, `sources`, `claims`, `nodes`, `edges`, `scenes`, `events`; narrated v2 plans also require nonempty `captions` |
| Profile | `renderer` is `remotion`, `hyperframes`, or `manim`; positive even integer `width` and `height`; positive numeric `fps` and `duration_seconds`; boolean `narrated` |
| Source | Unique `id`, nonempty `locator`, and `supports` summary. Locators may be official URLs or exact source-file paths and lines. |
| Claim | Unique `id`, `text`, `kind`, `source_ids`, and `rationale`. Kinds are `supported`, `inference`, `assumption`, or `illustrative`. Supported claims and inferences need at least one source. |
| Node | Unique `id`, `label`, `responsibility`, and nonempty `claim_ids`. Use stable IDs across all scenes. |
| Edge | Unique `id`, `from`, `to`, `label`, and nonempty `claim_ids`. Endpoints refer to nodes. Separate request and response directions. |
| Scene | Unique `id`, `start`, `end`, `caption`, nonempty `claim_ids`, and a `narration` object when narrated. |
| Narration | `text`, nonnegative scene-relative `offset`, `timing_basis`, `file`, and `duration_seconds`. Basis is `planned` or `measured`. Planned clips may have null file/duration. |
| Event | Unique `id`, global start `at`, nonnegative `duration_seconds`, `scene_id`, nonempty `node_ids`, `action`, nonempty `claim_ids`, `after`, optional `edge_id`, and nonempty `state_after` mapping participating node IDs to visible completion-state labels. |
| Caption | Global `start`, `end`, and nonempty `text`. Cues must be ordered, positive-length, nonoverlapping, and inside the video. Timing validity does not prove speech alignment. |

All timing values use **seconds**, not frames. Scene windows are contiguous
half-open intervals `[start, end)`, beginning at zero and ending at the declared
duration. Events occur inside their named scene. List events chronologically;
equal timestamps retain list order. A packet event includes both edge endpoints
in `node_ids`. `action` describes the visible behavior and its system meaning.
An event finishes at `at + duration_seconds`, no later than its scene end.
An `after` dependency must finish before or exactly when the dependent event starts.
Apply `state_after` at completion, never on departure. Simultaneous independent
events may overlap. For conflicting completion states, the later completion wins;
equal completion times retain list order. State labels support review; they do not
constitute a formal model of the architecture.

Version 1 remains valid. Its omitted duration means an instantaneous event and
its completion-state labels and captions are optional. Do not reinterpret a v1
timestamp as a measured motion interval. Upgrade a plan deliberately by declaring
actual durations and completion states, then checking the dependent timings.
The [reference fixture](../assets/reference-video.json) demonstrates version 2;
the older cache example remains a version 1 compatibility fixture.

Claims about a real architecture need inspected evidence. An illustrative node
or topology must cite a claim labeled `assumption` or `illustrative` with a
specific rationale. Labels alone do not prove that a claim is supported.

## Planning and audio retiming

The [example](../assets/cache-miss-example.json) is a 90-second **plan** with no
generated voice. Its times are editorial estimates. The ordinary check must pass;
the ready check must fail until narrated scenes have measured clips.

Before animation, save one PCM WAV per narrated scene under the project. The
checker resolves clip paths relative to the spec and rejects paths outside that
project directory, including symlinks that escape it. Convert another audio format
to PCM WAV with the available media tool before checking. This makes duration
measurement reproducible without a Python media dependency.

Set each clip's `file`, measured `duration_seconds`, and `timing_basis: measured`.
Read its duration from the generated audio, not a word-count estimate. Recompute
scene windows and dependent event times. Keep word/phrase alignment as a separate
artifact when the voice engine supplies it; it is not validated by this checker.
Preserve original WAVs and cues after review. The measured clip duration can be
exact while word cues remain model estimates. Check those cues against the audio
before claiming word-accurate reveals or captions. Retiming must preserve complete
native speech, including any partial final frame.

Resolve the skill directory from the installation, then run:

```bash
python "$SYSTEM_DESIGN_VIDEO_SKILL_DIR/scripts/check_video_spec.py" video-spec.json --ready --json
```

The check is read-only. Exit 0 means the declared contract passes; exit 1 means
input, structure, or audio verification failed. JSON output contains `valid`,
`stage`, and `errors`. `stage` is `plan` or `animation-inputs`, never final-video
certification. Clip duration tolerance is 0.1 seconds. Scene and event boundary
checks allow only floating-point rounding, not a missing frame or a story gap.

Silent projects set `narrated: false`. They need readable on-screen explanations
and the same architectural and event checks, but no voice clips. Do not use that
setting to conceal a failed narration requirement.

## Renderer and review obligations

Use stable IDs to map nodes, edges, and events to renderable elements. Describe
state changes explicitly in event actions or add renderer-owned state data. Do
not infer unrecorded messages or reverse an edge for a convenient layout.

Sample immediately before and at the start, during travel, and immediately before,
at, and after completion, plus dense motion and every scene boundary. Convert
seconds to the first frame at or after that time, clamped to the video range.
Before completion, do not expose the resulting state. At completion, show
`state_after`. A positive-duration packet must travel along its directed edge;
one frame after departure is not assumed to be its arrival. The production
helper emits these frame indexes for the chosen frame rate. Inspect final-frame
and next-scene behavior when completion falls exactly on a scene boundary.

Final media checks are separate. A structurally valid spec does not prove that
audio reached the MP4, that subtitles align, or that a viewer can read the labels.
Keep review rounds, hashes, actual playback coverage, encoded measurements, and
delivery receipts in the [production record](../assets/production-record.md), not
as invented certified fields in this schema. Follow
[production review](production-review.md) for those evidence-based verdicts.
