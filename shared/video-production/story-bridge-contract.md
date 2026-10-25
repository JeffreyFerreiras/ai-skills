# Story bridge production gate

A story bridge briefly leaves architecture-diagram presentation to introduce the
next idea through a recognizable situation, visible event, new understanding and
motivated return to the mechanism. Chalk/neon styling may remain. Relabeled boxes,
server housings, furniture around a diagram, before/after diagram states, fades,
wipes and camera moves alone fail. Do not add a scene-type quota or rush speech.

## One record, three different checks

Keep `scene-bridge.json` in the episode project. Paths below are relative to that
project, not to the installed skill. Its `schema_version` is 1. Required fields:

| Field | Meaning |
| --- | --- |
| `episode_id`, `creator_id`, `director_id` | Actual episode and bound identities. The director is a non-author. |
| `fps`, `frame_count` | Planned frame timing initially; retime to actual audio/render before encoded evidence. |
| `references.style`, `references.storytelling` | Separate reference responsibilities. Each has `status` (`inspected` or `unavailable`), exact `locator`, and concrete `observations` or access limit. Inspected references include `timecodes`. Style pixels do not prove storytelling. |
| `bridges` | Nonempty array of dedicated interior story scenes. Each has a unique filename-safe `id`; `visual_form` is `contextual_story`, `visual_metaphor` or `illustrative_cutaway`. A label alone is not proof that pixels meet the definition. |
| Each bridge | `context`, `subject`, `story_action`, `incoming_idea`, `next_idea`, `semantic_link`, `continuity_anchor`, `return_to_explanation`; distinct `incoming_scene_id`, `scene_id`, `next_scene_id`; integer `start_frame`, exclusive `end_frame`. Specify the actual situation/event and learning connection, not merely attractive assets. |
| `sample_frames` | Exact `before`, `entry`, `action`, `exit`, `after` frame indexes. Before precedes the bridge; entry/action/exit are ordered inside it; after is in the following scene. No hero-still substitution. |
| `implementation` per bridge | `source` artifact with project-relative `path` and SHA-256, exact render-function/call-site `locator`, optional `assets` artifact array. This maps the plan into actual scene code or an exported editable source, not an `implemented: true` assertion. |

Use the bundled `scripts/scene_bridge.py` in every new episode. Run the following
commands from the episode project, resolving `VIDEO_SKILL_DIR` from the installed
skill. No new approval checkpoint is added; the existing stage gates own these
receipts and their shared maximum of three total review rounds.

```bash
python "$VIDEO_SKILL_DIR/scripts/scene_bridge.py" check scene-bridge.json --stage plan
python "$VIDEO_SKILL_DIR/scripts/scene_bridge.py" check scene-bridge.json --stage implemented
python "$VIDEO_SKILL_DIR/scripts/scene_bridge.py" capture scene-bridge.json outputs/final.mp4 evidence/bridge-current --output scene-bridge-encoded.json --project .
```

The plan check cannot establish implementation. The implemented check requires
existing hashed source/assets but cannot prove the renderer executed them.
Capture probes the actual encoded frame rate/count and extracts full-size PNGs
at the five declared phases. Use explicit `--ffmpeg`/`--ffprobe` paths when needed.
Evidence/output paths must be new; capture never overwrites. A failed partial
capture is not a receipt or approval. Reconcile its known state before retrying.
The created record has no approvals and is not eligible for delivery.

## Blind encoded acceptance

Give a fresh logical reviewer only the audience-visible encoded samples and
temporal evidence, without the director's plan or intended interpretation.
Require a first reading of the situation, event, new understanding and return.
Save its output before supplying creator intent. Only then compare it with the
technical claims and handoff. A second, distinct visual reviewer checks the
story presentation, readable holds, phone labels, bottom captions and contrast.
Neither reviewer may be the creator or director. Preserve all other review roles.

Store the blind first reading as a hashed JSON artifact with `reviewer_id`,
`input_scope: "audience_evidence_only"`, current `video_sha256`, and `frame_hashes`
mapping every `bridge-id:phase` to the captured PNG hash. Its `observations` maps
each bridge ID to nonempty `situation`, `visible_event`, `new_understanding`, and
`return_to_mechanism`. These are observed readings, not production instructions.

The Supervisor attaches returned reviews to the encoded record's `reviews` array,
never inventing their conclusions. Each review has `role` (`visual` or `logical`),
distinct `reviewer_id`, `decision`, `candidate_digest`, `coverage`
(`sampled_frames` or `continuous_playback`), all `inspected_frames` keys, and
per-bridge `observations`. Both must explicitly accept `story_scene_present`,
`introduces_next_idea`, `continuity_clear`, `caption_safe`, and describe the
`observed_story_action` and `observed_connection`. False/missing checks block.
The logical review includes `audience_first_reading` plus a `first_reading`
artifact reference to the saved blind result. Generic `APPROVE` text is insufficient.

`candidate_digest` is SHA-256 of UTF-8 JSON for the entire record excluding only
`reviews`, with sorted keys, compact separators, no ASCII escaping and no
nonfinite numbers. Review files use `{ "path": "...", "sha256": "..." }`.
Do not change candidate content while reviewers work. Current tool/frame hashes
and blind observations must remain available with the immutable stage evidence.

```bash
python "$VIDEO_SKILL_DIR/scripts/scene_bridge.py" check scene-bridge-encoded.json --stage encoded --project .
```

The encoded check verifies actual source/video/frame hashes, re-decodes each
sample from that exact movie, checks observed role decisions and binds the blind
reading to those samples. For local `video-spec.json` projects, `video_project.py
media-check` also requires `--bridge-record scene-bridge-encoded.json`; bridge IDs,
neighboring scenes and frame windows must match the render timeline. A media
probe/decode pass without this gate is technical evidence only.

The checker cannot interpret images or authenticate a person's claimed viewing.
The Supervisor must verify the independent dispatch and actual inspection.
Describe sampled frames versus continuous playback/listening honestly. A false
semantic approval is not made true by this validator. Synthetic regression tests
exercise rejection and provenance mechanics; they never certify a real video.

## Reject, repair, or stop

Reject story claims that still read as architecture boxes, a diagram on a desk,
or a generic effect, even when they match the plan exactly. Review against the
original user requirement and reference narrative, not just Writer intent.
Repair the plan and implementation together; re-capture the actual revised
encode and obtain affected current-digest reviews within the remaining stage
budget. Material source/timing/video changes invalidate bound reviews. Preserve
old receipts and consumed rounds, including on resumed work or renamed exports.
Missing evidence or an unresolved failure at the cap blocks dependent acceptance.
Do not rerender completed videos merely because this skill changed.
