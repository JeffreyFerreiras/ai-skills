# Cloudflare cinematic case decomposition

An evidence-scoped reference for [Neon Chalk Systems](neon-chalk-systems.md).
Inspected on 2026-10-25. Reuse the teaching decisions; do not treat the generated
video as the factual authority for the outage or a template to copy unchanged.

## Source identity and inspection limits

Public trimmed video: [Cloudflare cinematic reference](https://youtu.be/kkVva-kh3wc).
The local trimmed master uploaded to YouTube is 336.619683 seconds, SHA-256
`072f9a5bb0e699043fda83b55e460d43f2ddc866d270927630f1140ab4f66264`.
This hash identifies the upload master, not YouTube's re-encoded playback.
The inspected original local MP4 is 339.847256 seconds, 1280x720, 30 fps, SHA-256
`9fe971c988d350ce6da26eba5320582d44d9f6d43480a791d80aec538c9e0daf`.
The public copy removes the closing Gemini Notebook slate; it is a distinct file.
The locators below refer to the original timeline, before that removed ending.

Evidence: four saved contact sheets with 48 requested sample positions from
0 through 339 seconds, plus full-size frames at 10, 60, 150, 215 and 320 seconds.
The saved technical report records a successful complete decode. Narrative
analysis also uses a machine transcript explicitly marked **not verified
verbatim**, with approximately 25-second chunk boundaries rather than word cues.
The transcript can support an approximate argument map, not exact quotations or
sentence-level screen synchronization.

No perceptual audio listening or continuous playback was available for this
inspection. Stills establish visible composition and state differences, not
motion, transitions, dwell quality, cadence or prosody. Approximate ranges below
group sampled teaching beats; they are not a verified cut list. Automated scene
change candidates are not authoritative semantic boundaries. No full QA pass or
phone-readability approval follows from this record.

## Observed visual vocabulary

- Dark blue-charcoal textured backgrounds, chalk-like contours, warm orange
  accents, white type, cyan request paths and violet configuration elements.
- Two related presentation modes: dimensional illustrated people, devices and
  server fleets; sparse, mostly flat explanatory diagrams. Both retain the
  dark field and outlined objects.
- Browser, processing node, museum origin and configuration document act as
  recurring anchors. Success and failure use labels or symbols as well as color.
- Large contextual subjects alternate with small centered technical assemblies.
  Empty canvas is common; several technical labels remain small despite it.

## Approximate scene map

The middle column describes sampled pixels. The last column interprets their
teaching role in light of the approximate ASR, rather than claiming exact spoken
alignment or observed animation.

| Original time | Visible samples | Teaching role |
| --- | --- | --- |
| 0-20s | Person at a desk, failed browser; angled error tablet at 10s. | Start with fictional Sam's museum-hours goal and the puzzle of a healthy origin whose page does not load. The tablet's precise error code is a caveat below. |
| 20-30s | Server corridor; paired user-data and system-instruction paths at 28-30s. | Move from personal stakes to infrastructure, then distinguish two causal pathways. |
| 40-70s | Browser, Cloudflare node and origin in a lower cyan route; internal assessment module; upper violet pipeline at 70s. | Establish the normal request path, explain features and add the separate configuration input. The successive stills support staged buildup, not a claim about reveal motion. |
| 80s | Perspective view of a large server fleet and connecting routes. | Widen the scale after explaining a single receiving node. |
| 90-120s | Broader metadata visibility, query, central file generator and oversized output appear in successive samples. | Trace how an old query assumption meets a changed visibility boundary and produces duplicate features. |
| 130s | Museum website with opening hours and a visit button. | Contextual reset: museum content has not grown; the changed object is internal configuration. |
| 140-160s | Feature module capacity; side-by-side oversized configuration at 150s; illustrated exception tablet at 160s. | Make the receiving limit concrete, then connect unexpected input to failure handling. |
| 170-190s | Central pipeline with changing green/red edge states at 170/175/180s; world map at 185/190s. | Explain alternating working and bad revisions, then distribution and impact scale. These are sampled states, not verified motion or exact deployment times. |
| 200-220s | FL2 failure with server-error callout; older proxy, bot score 0 and conditional customer rule; healthy museum origin remains visible. | Compare distinct symptoms under the same oversized input. Preserve the rule condition for older proxies. |
| 225-230s | Clean checked object beside tangled orange warning paths. | Visual synthesis of how a configuration flaw creates downstream complications. The metaphor is illustrative, not a literal topology. |
| 240-270s | Return to configuration/request diagram; numbered stop, restore and restart states; 14:30 UTC main-recovery card; fleet view. | Explain recovery order and separate main recovery from clearing all downstream effects. |
| 280-300s | Explicit later-follow-up heading, 1 May 2026; validation, staged canary and retained fallback panels; engineer illustration. | Mark the historical jump and connect reported safeguards to the failure mechanism. |
| 310-336s | Return to desk/person; causal recap text; server-error display; final validation/rollout/backup checklist. | Close the opening question, then generalize the lesson. Recap styling and incidental labels need care. |
| 337s onward | Closing Gemini Notebook slate. | Removed from the public trimmed copy. |

## Narrative and rhetorical structure from approximate ASR

The opening asks why a working destination cannot serve a simple human goal.
It then explains the normal path before introducing the independent configuration
path. Plain-language meaning precedes terms such as origin, feature and metadata.
This reduces the amount of new vocabulary needed for the eventual failure.

The central argument follows a causal chain: broader visibility exposes an old
query assumption, extra metadata becomes duplicate features, the generated file
exceeds a receiving limit, and proxy behavior determines the resulting symptom.
The museum-page reset separates customer content from the internal changed input.
The fleet and map views explain amplification after the local mechanism has been
established, rather than making network scale the original cause.

The comparison of FL2 and older proxies avoids a single universal failure story.
Recovery reverses the causal path in an ordered sequence. The later safeguards
are introduced with a separate date, then the return to Sam restates the chain
and answers the opening puzzle. This is a useful stakes, normal behavior, changed
input, failure, recovery and synthesis structure; its length and scene mix are
not universal requirements.

## Factual anchors and scope to preserve

Use Cloudflare's [18 November 2025 outage report](https://blog.cloudflare.com/18-november-2025-outage/)
for the historical mechanism, proxy differences and recovery times, and its
[1 May 2026 follow-up](https://blog.cloudflare.com/code-orange-fail-small-complete/)
for later company-reported safeguards. These official sources take precedence
over generated narration, illustrative graphics and machine transcription.

- The metadata query lacked a database filter. Wider visibility exposed extra
  metadata; duplicate features inflated the file beyond the 200-feature limit.
  Do not invent a `60 x 2` calculation or treat schematic row counts as data.
- FL2 experienced a panic and 5xx errors. Older FL assigned an incorrect zero
  bot score; blocking depended on a customer's rule. Do not flatten these into
  a universal blackout or claim every request failed.
- Main recovery was at **14:30 UTC** on 18 November 2025; full restoration was
  at **17:06 UTC**. A main-recovery title alone is not full-service restoration.
- The safeguards in the **1 May 2026** follow-up are later company-reported
  changes, not controls established as operating during November 2025, and not
  guarantees that no later outage can occur.
- Sam and the museum situation are explicitly illustrative. Generated screens,
  server layouts, paths and clocks do not establish additional historical events.

## Keep the useful choices; repair the weaknesses

**Keep:** a recognizable human goal; ordinary meaning before jargon; normal
behavior before perturbation; separate request/configuration lanes; persistent
objects; concrete matched comparisons; contextual reset and return; recovery
order; an explicit later date. Give progressive stages enough time, with scene
changes tied to the teaching purpose rather than a variety quota.

**Q1, factual overlay risk:** the 10-second tablet says `ERROR 522: CLOUDFLARE
TIMEOUT`. Cloudflare defines
[522 as an origin connection timeout](https://developers.cloudflare.com/support/troubleshooting/http-status-codes/cloudflare-5xx-errors/error-522/),
which does not match the FL2 panic/5xx mechanism explained later while the origin
stays healthy. Use an
accurate sourced symptom or a generic error display in new work. An attractive
transition illustration still requires factual review.

**Q2, label readability:** the 40-120-second diagrams and later rule/configuration
views use small labels, including the behavioral-traits and query text. Negative
space alone does not make these readable. Enlarge the main mechanism and its
conditions, reduce incidental text, and check the intended viewing size.

**Q3, recap continuity:** the 310-second sample puts black type on white blocks
over the illustration; the 320-second sample uses large white recap type but
also contains distracting small annotations. Retain the useful return to Sam
while keeping typography and semantic emphasis consistent. Do not copy incidental
generated text as meaningful system documentation.

**Q4, caption evidence:** these samples do not establish reliable bottom captions
or verified speech cues. Recap text and diagram labels do not substitute for
captions. The coarse machine transcript must not be promoted to verbatim aligned
subtitles without verification.

For a 9:16 adaptation, follow the scene-aware guidance in
[Neon Chalk Systems](neon-chalk-systems.md#vertical-adaptation-of-an-existing-wide-video).
Preserve both causal lanes, the healthy origin and the older-proxy rule condition.
Enlarge or restate labels, identify added learning cards as editorial summaries,
and retain native audio and duration when the task is a presentation conversion.
