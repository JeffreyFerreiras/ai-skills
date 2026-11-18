# Jeffrit chalk TikTok series

Canonical workflow for Jeffrey's chalk-style technical explainer TikToks. Use it when the request is one of these videos, a continuation of one, or a cover, caption, or repair for one. The recent episodes this was learned from are thread-pool starvation, retry storms, health checks, cartesian explosion, and transactional outbox.

This series selects the chalk treatment, the Jeffrit bust, and a default end card. Do not ask for a style again. A different brief still uses the style picker and the general defaults in `SKILL.md`. Do not force this series onto a landscape Remotion lesson, a NotebookLM packet, a silent animation, or a video whose style was explicitly chosen as something else.

Where this page names a number, that number is the series rule. The worked episode is [transactional outbox](examples/transactional-outbox/README.md). Its measured values show what one accepted master actually did. They do not loosen the rule.

## What changes from the general defaults

- Length is under 2 minutes (120 seconds), including the laptop close and the full ending. Do not use the general 180-second ceiling for this series.
- Loudness is -14 LUFS integrated, true peak -2 dBTP or lower, on the encoded master. Do not master this series to -18 LUFS.
- Narration is one TTS take per sentence, assembled with measured gaps, using the voice rotation below. Do not use one continuous take for this series.
- `am_fenrir` runs at speed 1.20. Other lessons stay at native speed 1.0 unless their own record says otherwise.
- Use the [series end card](#end-card) by default.
- Deliver the clean master with the TikTok cover and a caption-only message by default. Honor a video-only request.
- Jeffrey uploads to TikTok himself. Never publish, and never upload, without an explicit approval that is separate from delivery.

Shared procedures stay in force: sourced claims, the review loop, [teaching opening](teaching-opening.md), [hook development](hook-development.md), [audio measurement and repair](audio-and-delivery.md), [cover originals](cover-style.md), and [diagram QA](diagram-qa.md). This page adds the series contract. It does not add a fourth review round.

## Workflow

Move one stage at a time. Update the episode tracker at every stage, including a rejected sample and a blocked publish. The header lives in [episode-tracker.csv](../assets/episode-tracker.csv). Keep that tracker in the configured production home, not inside the installed skill, and copy the header when a home does not have one yet. Record hashes for the script, sample, master, and any posting assets produced. Do not invent a status.

1. **Topic.** Read the tracker. Propose 3 options that are not repeats of the episodes named above, unless Jeffrey asks to revisit one. Say which one you recommend and why it can be taught in one concrete example. Wait for the choice.
2. **Sourced script.** Write the script from official sources. For a .NET topic, start with Microsoft Learn. Include compiling C# for the bad path, the fix, and the trade-off the narration states. On-screen cards may be short excerpts. The episode still needs a program that compiles and exercises those claims. Run the independent script gate, then get Jeffrey's script approval. Do not generate narration before both.
3. **Short sample.** Render the hook through the first problem beat only. Use the chosen voice, the real gaps, the chalk laptop, and the first causal failure. Stop before the fix. For transactional outbox, that beat is the crash and the lost event. Provide the sample for review before any full render.
4. **Full render.** Start only after the sample review. Retime the picture to the measured audio. Run the QA gates below on the encoded master.
5. **Deliver.** Hand over the clean master, `QA-REPORT.md`, and `SHA256SUMS` with any posting assets in scope. When included, deliver the 1080x1920 cover and give the caption in its own message.
6. **Publish.** Stop. Jeffrey uploads to TikTok himself. Delivery is not approval to publish. Do not upload, schedule, or post.
7. **After publish.** Archive and clean working copies. Send lessons to the destinations Jeffrey has explicitly authorized.

Planning may continue while a choice is pending. TTS, the sample render, and the full render may not.

## Tracker

Columns: `episode_id`, `title`, `stage`, `voice`, `topic_options`, `chosen_topic`, `source_urls`, `script_sha256`, `sample_sha256`, `master_sha256`, `cover_sha256`, `caption_sha256`, `qa_report`, `publish_approval`, `uploaded_by`, `archive_status`, `lessons_drive`, `lessons_vault`, `updated`, `notes`.

Use these stage values, in order: `topic`, `script`, `script-approved`, `sample`, `sample-approved`, `rendered`, `delivered`, `publish-approved`, `published`, `archived`, `lessons-recorded`.

`uploaded_by` stays empty until Jeffrey confirms that he uploaded that file. `publish_approval` records his explicit yes, with the date, and is still not an upload. `lessons_drive` and `lessons_vault` are recorded separately. One of them being done does not complete the other.

## Script

Teach one mechanism. Start with the plain-language concept and objectives. Use
[episode storytelling](episode-story.md) to carry the teaching promise through
the same concrete case, the fix, and its trade-offs.
Show what changes for that case and what the fix costs. Name real costs that the
source supports, such as duplicates, delay or cleanup. Do not invent latency,
guarantees, or components.

Spoken words and on-screen code follow different spelling rules. Say the full name first. On screen, code keeps the real identifiers.

- Say "Entity Framework Core" the first time, then "Entity Framework". Do not say "EF Core".
- Say "sequel" for SQL. The code keeps `SQL`.
- Say "Save Changes" if that is the spoken form. The card keeps `SaveChangesAsync`.
- Captions follow the spoken canonical narration, not the code identifiers.

No em dashes and no en dashes in the script, captions, cover, on-screen text, or chat copy. Hyphens inside a word such as "at-least-once" are fine. The reference renderer rejects both dash characters in any string it draws.

No scene id and no track name on any copy, including drafts, contact sheets, and review stills. Refer to a moment by timestamp (`mm:ss`). Scene names may exist in production notes and in evidence filenames that are not handed over as the video. The pixels of a delivered or draft video stay clean. Deliver the clean master only. Do not also send a labeled draft, a burned-in scene cut, or a contact sheet as the video.

The transactional outbox script, canonical narration, and the three C# cards are the worked example. Each card line is at most 34 characters so the type-in stays readable at phone size.

## Sample

The sample is the hook through the first problem beat. It includes actual narration at the intended voice and speed, a causal state change, readable captions, and the laptop opening with no hands. A title card, a still, or a sample from another episode does not count.

Do not render past that beat until the sample has been reviewed. An authorization to produce the episode is not authorization to skip the sample. After the sample is accepted, do not regenerate it on resume if its bytes and review record are unchanged.

## Narration

Rotate only these approved voices. Take the next one that the previous episode did not use, and record the choice in the tracker. Do not add a voice, a Chatterbox sample, or a speed without an explicit choice for this episode.

- Kokoro `af_bella`, female, at native speed 1.0, on the full-precision model. The int8 weights sounded whispery on this series. Do not use the int8 OpenVINO IR for `af_bella` here. This does not change the tested int8 pin for other lessons.
- Chatterbox, male, sample 1.
- Kokoro `am_fenrir`, male, at speed 1.20. That setting is about 171 words per minute overall. The transactional outbox master, spoken by Fenrir at 1.20, measured 166.6 wpm over the file, 176.7 wpm over the speech span, and 214.3 wpm articulation, on 251 words. Do not time-stretch a take to force 171. Change the script, or regenerate a sentence, if the pace is wrong.

Record one TTS take per sentence. Keep the original WAVs. Assemble them with silence you insert, then measure the joins on the final MP4, music and SFX included. Stem gaps are the assembly target. The MP4 measurement is the one that passes or fails.

- Between sentences: about 0.42 s, and every sentence gap on the final MP4 must land from 0.35 s to 0.45 s.
- Between paragraphs: about 0.68 s.
- The outbox stems were sentence gaps 0.38 s to 0.45 s (median 0.42 s) and paragraph gaps 0.68 s. The same master, measured with VAD on the MP4, was sentence min 0.352 s, median 0.416 s, max 0.544 s, and paragraph min 0.64 s, median 0.704 s, max 0.768 s. The 0.544 s sentence gap is outside the band. Treat a gap outside the band as a finding. A stem that looks right does not close it.

Do not put a whoosh, or any other SFX hit, inside a sentence gap or a paragraph gap. If a cut falls in a pause, the pause stays quiet.

No spoken acronym the viewer has to decode. Spell the TTS line the way it should be said. On-screen code keeps the real name.

Run speech-to-text on every sentence of the final MP4, not on the dry stems. Use two Whisper sizes and more than one decode. The reference scripts use faster-whisper `small.en` and `base.en`, beam 5 and greedy, with `condition_on_previous_text` off, on the mixed audio. If the encode ends before an expected sentence ends, name that sentence as truncated or missing, keep the report, and fail the check. Do not drop the rest of its paragraph and exit clean. Regenerate any take whose mishear changes the meaning, then remeasure that sentence. Meaning-changing misses already seen: "polls" heard as "pulls", "isn't" heard as "is an", "at least once" heard as "at least one's", and a soft "The" dropped at the start of a sentence. Homophones that keep the meaning in context are acceptable, such as writes/rights, sent/scent, and/an, hear/here, or/are. The outbox QA left "polls"/"pulls" as a residual on some paragraph decodes. That residual is not a waiver. Regenerate it.

A Whisper hallucination on one short clip, absent from the other decodes and from the waveform, is a decoder error. Say so. Do not treat it as words that are in the audio.

## Picture

1080x1920 vertical. The worked example is 24 fps. Keep one frame rate for the whole master, including the ending. The general planning default elsewhere is 30 fps. Do not mix them inside one file.

Dark chalkboard. The episode board is a textured blue-black field. The standalone end-card clip uses flat RGB 17, 25, 29. Match the episode board through the crossfade, then let the card fade.

Match drawn lines to the chalk artwork across clocks, arrows, outlines and state
edits. Use dry edges and restrained pressure and opacity variation. Keep grain
fixed to each stroke or moving surface as it reveals or moves. Where the
renderer generates texture, use repeatable seeds across render workers.

A physical laptop opens and closes by itself. No hands, arms, fingers, or forearms anywhere in the frame, including reflections and asset names. The reference check rejects person assets whose names match hand, arm, finger, or forearm.

Use the opening motion to introduce the first teaching image. When adding a
lead-in to an accepted cut, shift the body, narration, captions and timed
effects by one measured offset. Keep the spoken hook intact and inspect the
encoded join.

Chalk people are detailed and front-on. The developer is the Jeffrit gigachad bust: the whole bust from [jeffrit-bust.png](../assets/brand/jeffrit/jeffrit-bust.png), about 340 px wide on the 1080 frame. Do not paste the face onto another body.

Give one layer ownership of the burned-in spoken captions. In a repair, account
for any captions already burned into the source before adding an overlay.
Keep only numbers, code, identities and states the viewer needs. Remove
headings and labels that repeat narration. Captions stay in the caption band.
In the reference renderer, scene content ends above y 1560 and caption glyphs
sit from y 1570 to y 1780, with side margins of 36 px.

C# cards use a real lexer (the reference uses Pygments `CSharpLexer`), whole tokens, and monospace positions. Lines stay at or under 34 characters. Let the viewer read the code. Do not shrink the card to win back time. Type the code in as it is spoken.

Compute typing progress from the exact source, including line breaks and
indentation. Derive displayed text, syntax colors and cursor position from
the same visible prefix. Check exact prefixes at newlines and token
boundaries, and verify the encoded frames around reading holds and completion.
If keys are audible, trigger them from visible non-whitespace reveals and
check the key stem for stray taps during holds and after completion.

Show causal data flow. A check appears only after the commit that earned it. A subscriber reacts only after the message arrives. A duplicate is dropped only after the consumer has recorded the first copy. Animate the route. A camera move does not replace that change.

Give arrows smooth routes and clearance. Prefer proportionate open heads with
clean joins aligned to the route's end tangent. Use local emphasis when a
connector adds no spatial information.

Enlarge small props and log lines until they read on a phone. Inspect encoded frames at 360x640, not only at 1080x1920.

## Sound

Use music and SFX. Keep rumble and buzz out. High-pass the music at about 230 Hz and the SFX at about 300 Hz.

Master the encoded file to -14 LUFS integrated and a true peak of -2 dBTP or lower. The outbox master measured -14.0 LUFS, -2.8 dBTP, LRA 2.8 LU, with ffmpeg `ebur128` on the AAC inside the MP4. The reference [master.py](../scripts/reference/transactional-outbox/master.py) reaches that with an iterative gain and a limiter, then measures the AAC. It writes the final master filename only after a pass is within 0.05 of -14 LUFS and at or below -2 dBTP. Ten misses keep the measurements and exit nonzero. The two-pass `loudnorm` recipe in [audio and delivery](audio-and-delivery.md) is the other accepted path when its targets are I=-14 and TP=-2 or lower. Do not aim this series at -18.

When the series end card is used, music resolves on a final chord under it. In the outbox build the bed swells from 0.4 s before the card to the bust pop, then hands over to a C add9 chord, high-passed at 230 Hz, which reaches digital silence 60 ms before the file ends. Do not cut the music off on the last frame of the lesson.

## End card

By default, end with the Jeffrit chalk gigachad end card, crossfaded in after the laptop closes. Honor an explicit alternate ending or preserve the approved ending already in the cut. The laptop close in the outbox example is 1.55 s, then the card.

For the series end card, use [endcard.py](../assets/brand/jeffrit/endcard.py). `EndCard.LEN` is 2.9 s:

- 0 to 0.6 s: crossfade in from the previous frame.
- 0.3 to 0.95 s: the ring draws.
- 0.6 to 1.0 s: the bust pops.
- 1.02 to 1.95 s: "Jeffrit" writes on.
- 1.90 to 2.25 s: the underline.
- 2.2 s: jaw glint.
- The last 0.45 s: fade.

The only word on the card is "Jeffrit". Brand layout and the bust are described in [the brand kit](../assets/brand/jeffrit/README.md). The standalone [jeffrit-endcard.mp4](../assets/brand/jeffrit/jeffrit-endcard.mp4) is 3.375 s, silent, on RGB 17, 25, 29. Use it as a look reference. The episode timeline is the 2.9 s class, not the standalone file's duration.

## Cover

1080x1920 chalk. One headline, one diagram, and the gigachad bust. Also satisfy the shared [cover gate](cover-style.md): view both original covers, check a 240 px thumbnail, and get an independent cover review inside the existing three rounds.

Series geometry, which the original covers do not state:

- All key art sits inside the centered 3:4 band: 1080x1440, y 240 to 1680.
- Key art also stays clear of the bottom 250 px, so content ends above y 1670.
- Check three views of the same file: full 1080x1920, the center crop from y 240 to 1680, and a 240 px wide thumbnail. The hook, the diagram, and the bust must still be recognizable in the thumbnail and in the crop.

The approved outbox cover and the compose script that restacked an earlier cover into that band are in the worked example. Do not Python-edit an accepted cover in place. Compose a new file.

## Caption

One message. The message body is only the caption. No "Caption:" label, no file path, no QA note, no scene id.

The caption states the problem with concrete numbers, the fix, and the trade-offs. Hashtags are lowercase and sit on the caption. The outbox caption is the pattern: two writes that nothing ties together, the outbox plus one `SaveChanges`, at-least-once delivery, polling delay, and cleanup.

Do not use em dashes. Do not invent metrics.

## QA gates

Run these on the encoded master before delivery. Write `QA-REPORT.md` and `SHA256SUMS`. Record SHA-256 hashes of the delivered master, report, and any cover or caption included in the package.

- Automatic frame checks from the renderer: no scene or track labels, no limbs, captions inside the caption band, content outside that band, and the causal state checks for that episode. Keep a known-bad fixture that the check must reject. [Diagram QA](diagram-qa.md) is the geometry and code-card procedure.
- Contact sheet of the encode.
- Phone-size frames at 360x640 across the episode, including code, the laptop, a failure, the fix, and the actual ending.
- A strip of every transition, including the laptop close and end-card crossfade when used.
- Integrated LUFS, true peak, and the high-pass confirmation for music and SFX.
- Pause and wpm measurement on the final MP4, sentence and paragraph, plus the stem targets for comparison.
- The two-size, multi-decode STT report, with each meaning-changing miss either regenerated or still open. An open miss blocks delivery.

Record what was inspected and what was not. A self-check does not replace independent review. Listening, when it did not happen, stays pending.

## After publish

Wait until Jeffrey confirms the upload of that master.

Archive with the video-production-drive-cleanup skill. That cleanup is the Video Janitor step. Copy the accepted master, any delivered posting assets, QA report, hashes, and the source project into the configured system-design archive, read the copy back, and update the tracker with the archive result. Then clean only the working copies that skill allows. Do not archive or delete before publish. Do not treat cleanup as permission to post.

Keep proposed lessons in the authorized episode record when they are learned. Delivery does not wait on the write-up. Require explicit authorization for each destination and for any Git push. Treat upload confirmation as the trigger for archive and cleanup.

- When authorized, write a short retrospective, the next-run checks, and an evidence index to the configured Google Drive lessons document. Read it back.
- When authorized, process the same lessons into the knowledge vault. Follow the vault repository's branch and review rules for any authorized push.
- Apply the local production-lessons journal's own write rules.

Track the Drive and vault updates separately. Mark a destination pending if it lacks authorization or the write could not be completed. Keep Drive file IDs, vault paths, and machine IDs out of this skill.

## Reference scripts

The outbox render, audio, pause, speech-to-text, encode QA, and loudness scripts are the reference pipeline. Read [their README](../scripts/reference/transactional-outbox/README.md) before running one. They are the episode pipeline, not the supported spec checker beside them in the scripts directory. Narration takes, the laptop plate, and the thread-pool sound helper are not bundled.
