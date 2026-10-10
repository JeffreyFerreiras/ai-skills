# Production and review gates

Keep the production brief authoritative for presentation, not facts. Begin it with `PRODUCTION INSTRUCTIONS — NOT A TECHNICAL FACT SOURCE`. Link each factual claim to a passage and separate defining behavior from optional implementation. State whether the mechanism is a built-in feature, a library, an operational command or custom design. Do not present an illustrative algorithm as a product feature. Use small readable values, distinguish IDs/content/hashes and preserve intermediate states.

The narrative is orientation (what/why) → concrete scenario → definition/uses → normal mechanism → practical implementation → failures/limits → synthesis. A scenario-only opening needs an orientation. Every explanation needs its own useful composition, chosen for its mechanism. Reuse identities and local anchors, not one unchanged diorama throughout.

## Per-deliverable review loop

Each asset deliverable needs an independent subagent reviewer before its dependent production gate or delivery. Cover the production document, narration/storyboard, context art and concept diagrams, native audio/cues, captions, voiced sample, animatic, encoded master, phone copy and recovery package as they become deliverables. Related files may be reviewed together only when each asset is explicitly inspected and recorded; one final video review cannot substitute for earlier asset reviews. A self-check does not satisfy independence. If reviewers are unavailable, record the missing gate rather than inventing approval.

Use **one to three review rounds per deliverable**, never three by default. A round is an independent review of the current candidate; applicable technical and editorial reviewers can work concurrently within that round and must report separate verdicts. Revisions do not reset the counter. Give reviewers the exact revision/hash, purpose, source anchors, requirements, preceding accepted decisions and actual artifact. Choose review scopes suited to that asset: technical truth/contracts and editorial teaching/presentation remain distinct.

1. Review the candidate and record reviewer identity, round number, inspected artifact/timecodes, playback/listening or sampled scope, actionable findings and verdict. Stop immediately when no actionable findings remain; record the clean result without extra rounds.
2. Fix routine actionable findings internally, preserving accepted choices. Record finding ID, affected asset/timecode, impact, fix, revised hash and concrete verification. Send the repaired asset plus surrounding context to the independent reviewer only if another round remains. Routine revisions do not require repeated user approval.
3. After round three, stop the loop. Record remaining issues and inspection gaps explicitly. Unresolved critical issues block their dependent production/delivery gate and need the user's attention; do not silently approve, mark a clean pass or run a fourth round. Remaining noncritical findings stay visible as limitations, with the reviewer disposition and unmet requirements; a capped review is not automatically a passed gate.

Bring the user a material creative choice, unresolved critical finding or actual permission blocker. Continue other authorized work while resolving it. This loop organizes review; it does not grant publishing permission, establish unheard audio quality or waive required gates. Keep the asset review ledger in the current production document/tracker rather than introducing a separate orchestration system.

## Reviews before long rendering

The sample must demonstrate the mechanism, captions and transition with real narration, not just a pretty cover. Apply the bounded independent review, record feedback and the exact source revision, then deliver it with actual delivery evidence. Approval of a style still or silent test does not approve a narrated sample. An approved sample is not full-video acceptance. When requested changes are narrow, fix those dimensions while preserving accepted decisions.

When the user has already authorized full production, sample delivery plus passing internal gates permits continuing without another routine user checkpoint. Record the authorization and independent review evidence as the basis; do not label internal feedback as user feedback. If they explicitly reserved a sample decision, honor that instruction. Material creative decisions, unresolved critical findings and actual permission blockers still need their involvement. Full-video acceptance and publishing authority remain separate.

After the sample, review a native-paced animatic covering each explanation and challenging transitions. Do not compress a minute into a few seconds to assess pacing. Sampled temporal frames can establish visual sequence but cannot establish perceived spoken pacing. When continuous playback/listening is unavailable, record that evidence gap and arrange human playback instead of claiming the gate passed in full.

Editorial findings need timecode, observed issue, learning/viewing impact and focused repair. Check orientation, stakes, scene variety, progressive mechanism, matched narration/state, internal action, readability, restrained depth, transitions, stable pauses, continuity and cognitive load. Check that quality survives the complete runtime. Recheck repaired portions plus surrounding context within the deliverable's remaining rounds.

Technical findings need the violated contract and evidence: source support, state/event order, IDs/values, protocol/math assumptions, hashes, alpha/clipping/layer/camera behavior, geometry, captions, waveform, full decode, frame count/timestamps, dimensions/codecs and completeness. Tests of finite example states do not prove a general distributed-systems guarantee.

## Encoded review and delivery

Review the actual export after encoding, including a separate phone legibility check. Continuous watch/listen is preferred for pacing, pronunciation, awkward silences and engagement. Record sampled-frame timecodes/counts, temporal excerpts, playback/listening coverage, full-decode results and alignment method separately. No numerical score, ffprobe result or geometry test substitutes for editorial judgment.

Measure final master **and** phone integrated LUFS and true peak dBTP, with conservative headroom and a documented speech delivery target. Follow [encoded loudness QA](loudness.md): measured two-pass repair rather than blind gain, AAC peak correction and remeasurement, unchanged native voice/rate/cues, video bitstream/timing and distributed waveform sync checks. Independent technical measurement review and actual listening are separate gates; an acceptable PCM peak does not approve a final AAC export.

Tracker transitions require evidence: drafted → sample produced → sample approved → full rendered → QA passed → delivered → user-ready → published. Pending or failed gates remain explicit. A successful attachment send establishes delivery only. User-ready needs explicit full acceptance or a satisfied user-defined condition; posted needs publication evidence. Preserve legacy completed-topic labels, flag overlap and update existing attached copies when authorized and available.
