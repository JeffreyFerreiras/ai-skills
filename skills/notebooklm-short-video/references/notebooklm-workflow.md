# NotebookLM production source and activation

Read the selected format's SKILL.md for scope and pacing. This common procedure
is bundled into both NotebookLM skills so either works as a standalone install.
It does not require the local code-rendering or Kokoro pipeline.

## Establish the episode and its evidence

Read the existing tracker, aliases, episode manifest and latest user decisions.
Check for an already completed or generating output before adding sources or
starting another generation. Record the notebook/output identity and current
state. Preserve unrelated sources, user edits, names, sharing and existing videos.
Resolve the intended notebook and signed-in profile from actual connected state;
do not guess an account, create one or broaden access.

Separate four responsibilities: technical sources establish facts; one active
episode production source controls teaching sequence and scope; an inspected
style source supplies visual language; the separate short activation prompt
connects them. A production plan is not technical evidence. Inspect the actual
supporting passages and keep a claim/source ledger with precise locators,
historical scope, assumptions and illustrative values. Do not upload private
skill source, review transcripts, credentials or unrelated project files.

Before drafting, dispatch an independent example-fit advisor to compare 3-4
plausible scenarios for realism, causal visualization, audience fit, concept
overhead and novelty against recent episodes. Do not default to checkout/payment.
Use its strongest justified recommendation, then obtain the non-author
[scene director handoff](scene-direction.md). Retain useful chalk/neon elements
without confining every concept to the same diagram. Existing references and
history are documented in [reference history](reference-history.md).

## Create two distinct artifacts

Write the production document outside NotebookLM, using the user's established
file format where one exists. A readable UTF-8 TXT or Markdown document is a
portable fallback only if the current upload surface supports it. Start with the
exact episode/source title and **PRODUCTION INSTRUCTIONS - NOT A TECHNICAL FACT
SOURCE**. Include the audience and objective; format/duration intent; spoken
hook; context and what/why overview; teaching order; concrete example; script or
narration beats; storyboard; director's scene map and fresh transition;
technical guardrails and exclusions; source ledger; caption/voice intentions;
and output acceptance criteria. Keep instructions and citations out of spoken
copy. Avoid em dashes in audience-facing copy.

Make a separate activation TXT file that names the exact active production
source, exact selected style source, factual source set and requested output.
Keep it short and operational; do not paste the full storyboard or ask NotebookLM
to invent a second plan. A useful shape is:

```text
Create [the selected format and episode scope], at a natural teaching pace.
Use "[EXACT PRODUCTION SOURCE TITLE]" for sequence, example and scope.
Use "[EXACT STYLE SOURCE TITLE]" for visual language, with the episode plan's
explicit scene-director choices governing scene variety and the fresh transition.
Use the selected technical sources for facts. Preserve the plan's assumptions,
limits and exclusions. Start with its spoken hook, then context and the ordered
explanation. Keep labels readable and captions at the bottom where supported.
Do not narrate production notes, source names, citations or social metadata.
```

Replace all placeholders with the reviewed episode's actual names and choices.
The current director instruction supersedes the older all-diagram-only reminder
for new episodes. If a selected legacy style source conflicts, state the scoped
episode override in both artifacts. Preserve that source's identity and unrelated
content; do not silently rewrite a shared style file. Mark requested duration,
aspect, voice and captions as intentions until the actual controls/output prove
them. Do not force local Kokoro settings into NotebookLM prompts or promise exact
word-for-word narration or frame-accurate choreography.

## Review, upload and activate within requested scope

Freeze exact artifact hashes, source names/identities, scope and acceptance
criteria. Use one bound Writer and fresh independent reviewers. Technical and
narrative/beginner-comprehension approval of the document, hook and storyboard
precedes upload for production and generation. Check the director's SD1/SD2
criteria and the activation prompt's exact source references. Keep immutable
reports, findings and consumed rounds in episode working files, outside the
installed skill. Each deliverable has at most three total shared review rounds,
including the initial round; no counter reset after edits, renamed files,
regeneration, resumed tasks or reviewer changes. Settle the Writer, validate
current hashes and invalidate affected approvals after material revisions.
Missing required independent evidence leaves the dependent gate pending.

For an upload/generation request, prefer available supported NotebookLM tools;
otherwise use the host's supported browser controls and observed current UI.
Document-only work stops with the reviewed production document and activation
prompt. Never activate merely to test this skill. An upload-only request stops
after verified ingestion; it does not authorize generation.

1. Inspect existing sources and completed/in-progress video outputs. If a match
   exists, report/reuse it within the request rather than starting a duplicate.
2. Upload only the reviewed episode source and relevant authorized technical/style
   references that are missing. Reuse exact existing source identities. Retain
   older relevant plans as history where needed, but keep competing or superseded
   plans unselected; no destructive cleanup is required.
3. Verify ingestion/readability, exact source title, selected sources and supported
   video controls. Select the one active plan plus applicable style and factual
   sources; exclude unrelated episode plans. Do not infer source selection from
   successful upload alone.
4. When generation is requested, use the separate activation prompt in the
   supported video customization surface. Inspect the current format/length/style
   options; choose the requested supported option. Report material unavailable
   controls rather than promising them or silently substituting another product.
5. Record submission/output identity and state. On timeout or an uncertain action,
   inspect the same notebook/output before retrying. Never duplicate a job whose
   outcome is unknown. Stop on denied/unavailable access; keep the two artifacts
   ready for handoff and continue independent permitted work.

## Verify and deliver

Review the actual output independently for technical accuracy, plain-language
teaching, voice consistency, every scene's visual quality and fresh-context
logical continuity, including the director's SD3/SD4 checks. For logical review,
capture the audience-visible first interpretation before supplying intended
meaning. Inspect sampled frames and temporal evidence across the transition,
causal boundaries and cutback; state exactly which intervals were played/heard.
A contact sheet cannot establish continuous playback or listening. Keep all
roles within the stage's shared three-round budget; unresolved material findings
at the cap block dependent acceptance, not an automatic fourth generation.
If NotebookLM cannot repair a part, describe the limitation and available
authorized remedy without discarding useful outputs or regenerating blindly.

When a downloadable file is available, verify the actual dimensions, duration,
streams, decode and encoded caption/label readability using available local media
tools. Prompt settings and a thumbnail are not proof of the generated format.
Never claim a requested caption or voice was honored without inspecting it.

Follow existing episode naming/storage. If none exists, a proposed default is
`<Topic> - NotebookLM <Cinematic|Short> - Production.txt`, a separate
`... - Activation.txt`, and `<Topic> - <Full|Short>.mp4`; keep exact source titles
inside the manifest and activation prompt even if filenames differ. Store the
packet and QA in the episode's established working/source location and the
continuous video in the authorized destination. Verify transferred bytes when
files are copied. Do not scatter backups, overwrite accepted originals, change
sharing or automatically copy raw bundles to a share drive.

Deliver both input artifacts, the verified notebook/output link and any requested
continuous final, source/reference identities, actual measured format/runtime,
QA limits, review outcomes and next action. Keep planned, uploaded, generating,
generated, QA passed, delivered, user accepted and published states separate.
No new user checkpoint follows passed internal gates; honor explicit reserved
decisions. Publication still needs corresponding user intent and evidence.
