# Cover style and delivery gate

Apply when a cover is requested or already part of the episode deliverables.
This requirement does not authorize adding covers, regenerating completed media,
uploading private skill source or publishing anything. Cover style references
are separate from the storytelling references used for the video bridge.

Before designing, actually view both original images bundled in
`assets/cover-references`: [Safe Rollbacks](../assets/cover-references/safe-rollbacks-tiktok-cover.png)
and [Logical Clocks](../assets/cover-references/logical-clocks-tiktok-cover.png).
Their [manifest](../assets/cover-references/manifest.json) records identities,
unchanged byte hashes and provenance. Open pixels with an available image viewer;
filenames, IDs, text descriptions and a previous inspection are insufficient.
Keep these assets in the installed skill when synchronizing it.

Use huge, rough chalk lettering on a dark textured board, one compact expressive
illustration or coherent illustration group, a short honest hook and minimal
labels. Keep strong contrast and purposeful magenta/cyan or yellow accents;
colors alone do not carry meaning. The originals use technical icons within one
dominant illustration, so a technical subject is welcome. Reject thin clean
sans-serif lesson-slide layouts, small explanatory paragraphs, many competing
panels and cluttered implementation labels. Match the visual hierarchy, not an
unrelated title or exact illustration. Preserve episode meaning and phone-safe
margins. Avoid em dashes in audience-facing copy.

Create a thumbnail from the actual final cover, at 240 pixels wide with its
aspect ratio preserved, and inspect it without enlarging it. The hook and subject
must remain immediately recognizable. A full-resolution pass alone is insufficient.
Use the bundled helper in this skill root (`VIDEO_SKILL_DIR` means
`skills/tiktok-copy-cover`):

```bash
python "$VIDEO_SKILL_DIR/scripts/cover_style.py" capture outputs/cover.png --thumbnail evidence/cover-r1-thumb.png --output cover-r1.json --episode EPISODE_ID --creator WRITER_ID
```

Capture creates an unapproved record; use new paths and never overwrite old
evidence. Explicit `--project` is available. Thumbnails render with Pillow
(no FFmpeg); the record stores the Pillow decoder version for reproducibility. Record the producer's
actual inspections of both originals in `reference_inspections`: each entry has
`reference_id`, exact asset `sha256`, `viewer_id` equal to `creator_id`,
`pixels_viewed: true` and concrete `observations` about its hierarchy and lettering.
The two IDs and hashes come from the bundled manifest.

Give a non-author cover visual reviewer the actual full cover, thumbnail and both
original reference images. Require actual viewing and comparison. Return a
`cover_visual` review with `reviewer_id`, `decision`, current `candidate_digest`,
`inspected_images` mapping `cover` and `thumbnail` to their hashes, and
`reference_hashes` mapping both reference IDs to their hashes. Its `observations`
must explicitly accept `rough_chalk_lettering`, `one_dominant_illustration`,
`short_hook`, `minimal_labels`, `thumbnail_readable`, `accessible_contrast` and
`not_lesson_slide`, and describe `observed_hook` and `observed_illustration`.
The Supervisor attaches the actual returned result; generic APPROVE text or
author self-review cannot clear this gate. Review against the reference pixels,
not just conformity to the Writer's design plan.

`candidate_digest` uses the story-bridge encoding: SHA-256 of UTF-8 sorted compact
JSON for the entire cover record excluding only `reviews`, no ASCII escaping and
no nonfinite numbers. Capture leaves reference inspections empty. Complete those
before freezing the candidate and dispatching the reviewer.

```bash
python "$VIDEO_SKILL_DIR/scripts/cover_style.py" check cover-r1.json
```

Before cover delivery, run this check and verify the independent dispatch/viewing
evidence. It checks original asset hashes, current cover and thumbnail provenance,
and the recorded independent decision. It cannot recognize lettering quality or
authenticate a claimed inspection. A machine pass does not establish style by
itself. Missing capabilities or actual inspection leave acceptance pending.
Use the installed Pillow; do not install or change providers to test.
Keep all other applicable technical, beginner-comprehension and asset reviews;
this cover visual verdict complements them.

Carry findings through the existing shared maximum of three total review rounds
per cover deliverable, including prior attempts. No new counter, renamed-file
reset or optional user approval checkpoint. Material cover/reference changes
invalidate affected approvals; preserve superseded evidence and consumed rounds.
If a decoder/version change makes a thumbnail differ, inspect and recapture with
new paths and obtain a current review inside the remaining budget. Do not call
synthetic schema/thumbnail tests actual production-cover QA.
