# Production contract

Use `video-spec.json` when scenes share an architecture or have narration-driven
events. This is a renderer-independent input contract. The bundled checker
validates references, declared evidence, causal order, scene coverage, and audio
fit. Human review must still establish source truth, teaching quality, visual
readability, and whether the renderer obeys the spec.

## Fields

| Object | Required fields and meaning |
| --- | --- |
| Root | `schema_version: 1`, `question`, `audience`, `profile`, `sources`, `claims`, `nodes`, `edges`, `scenes`, `events` |
| Profile | `renderer` is `remotion`, `hyperframes`, or `manim`; positive even integer `width` and `height`; positive numeric `fps` and `duration_seconds`; boolean `narrated` |
| Source | Unique `id`, nonempty `locator`, and `supports` summary. Locators may be official URLs or exact source-file paths and lines. |
| Claim | Unique `id`, `text`, `kind`, `source_ids`, and `rationale`. Kinds are `supported`, `inference`, `assumption`, or `illustrative`. Supported claims and inferences need at least one source. |
| Node | Unique `id`, `label`, `responsibility`, and nonempty `claim_ids`. Use stable IDs across all scenes. |
| Edge | Unique `id`, `from`, `to`, `label`, and nonempty `claim_ids`. Endpoints refer to nodes. Separate request and response directions. |
| Scene | Unique `id`, `start`, `end`, `caption`, nonempty `claim_ids`, and a `narration` object when narrated. |
| Narration | `text`, nonnegative scene-relative `offset`, `timing_basis`, `file`, and `duration_seconds`. Basis is `planned` or `measured`. Planned clips may have null file/duration. |
| Event | Unique `id`, global `at`, `scene_id`, nonempty `node_ids`, `action`, nonempty `claim_ids`, `after`, and optional `edge_id`. Dependencies refer to earlier events. |

All timing values use **seconds**, not frames. Scene windows are contiguous
half-open intervals `[start, end)`, beginning at zero and ending at the declared
duration. Events occur inside their named scene. List events chronologically;
equal timestamps retain list order. A packet event includes both edge endpoints
in `node_ids`. `action` describes the visible behavior and its system meaning.

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

Sample at `event.at - 1/fps`, `event.at`, and `event.at + 1/fps` within the video
range, plus dense motion and every scene boundary. The image before an event
must not expose its result. The image after it must match the declared state.
Checking only the final frame misses intermediate collisions and causal errors.

Final media checks are separate. A structurally valid spec does not prove that
audio reached the MP4, that subtitles align, or that a viewer can read the labels.
