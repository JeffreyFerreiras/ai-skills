# Local narration

Read when choosing or setting up local TTS. Explicit voice choices and approved
recordings take precedence. This package supplies guidance, not model weights,
an inference runner, or permission for downloads or migration.

## Preferred engine and voice identity

Prefer MOSS-TTS Local Transformer v1.5 for new narration when the brief leaves the
engine open. Keep Qwen3-TTS as a fallback; record that decision and review its
representative sample before full production. Do not silently switch engines
within an approved performance or rerender completed videos.

The preference comes from listening to three volume-matched paired samples on
2026-10-12. The listener preferred MOSS's conversational delivery and natural
pauses; Qwen sounded monotone and abrupt in that comparison. This is a listening
preference, not a universal ranking. ASR content checks passed, but no agent
perceptual listening was claimed. ASR and signal checks cannot prove naturalness,
phrasing, or perceived pause quality.

When that voice is selected, condition on the approved original synthetic female
reference. Retain raw audio, exact transcript, hash, generation provenance, model
revisions, and inference settings. This is original synthetic voice conditioning,
not actor cloning. A seed alone does not preserve voice identity. Another brief
uses its own authorized reference rather than inheriting a prior episode's voice.
Qwen VoiceDesign can create an original reference and its matching Base model can
condition on it; record those roles separately.

Preserve native adaptive prosody. Give dense explanations room, emphasize
consequences, and move through connective material naturally. Do not turn a past
tempo value into a global speed rule. Generate connected clauses or paragraphs
within engine limits, then measure actual WAVs. For explicitly authorized tempo
or pause edits, retain raw sources and sample offsets, align the processed waveform
afresh, and renew listening checks.

## Continuous takes and sentence pauses

Independent scene synthesis and mixed retakes can change perceived identity,
timbre, or energy even with one reference. For a short sample, prefer one
continuous generation and segment visuals around that actual recording. If the
bounded take is too short or long, revise the script and regenerate the complete
take rather than rushing speech, padding excessive dead air, or splicing unrelated
voice takes. Preserve approved audio unless a change is authorized. Test a longer
narration strategy explicitly; do not assume unlimited text fits one generation
or that matching reference settings guarantee continuity across chunks.

One user-accepted 60-second sample used a single 145-word take with 54.24 seconds
of native narration, 126.958 ms of verified quiet-gap additions, and a 5.633-second
visual recap hold; the displayed figures are rounded. No speech stretching was
applied. The user's audition established acceptance of that repaired sample;
instrumented review alone did not.
These describe a bounded case, not a required script length or hold template,
universal engine capacity, or a listening pass for every future sample. Keep
native audio, pause edits, and the final visual hold as separate measured values.

Use clean audible pauses between sentences while keeping within-sentence speech
connected and naturally emphasized. Trial minimum gaps of 550 ms between
sentences and 650 ms at scene boundaries are configurable examples, not a locked
pace, fixed-WPM policy, or permission for global time stretching. A scene boundary
inside a sentence does not justify interrupting that sentence; move the visual
boundary or keep its narration connected. At coincident boundaries, choose one
intended pause rather than stacking both minimum gaps.

For authorized pause-only edits, identify actual waveform/sentence boundaries;
never cut inside a phoneme. Preserve the original PCM speech spans exactly and
write a derived output with a sample-offset edit map. Verify clean joins, unchanged
speech samples, actual pause lengths, and total duration. Update all downstream
word/phrase cues, captions, visual anchors, scene windows, and end holds from the
new waveform. Suppress word highlights during silence. Metrics locate candidates;
listening determines whether pauses or joins sound natural.

## Independent voice-consistency gate

Assign a dedicated non-author voice reviewer before final visual retiming and the
full render, within the existing narration asset's review budget. Inspect actual
delivery within phrases, between sentences, and across joins. Check rushing or
dragging, unnatural midphrase pauses, intelligibility/pronunciation, perceived
identity and timbre, energy, loudness continuity, and transitions to neighboring
material and the selected reference. Natural emphasis and semantic pauses should
remain; constant WPM or equal sentence lengths are not the goal.

Record each finding's phrase/scene, audio time range, observed issue, consequence,
specific repair, and both neighboring-join checks. Use timing, speech-region
rates, pause lengths, and loudness to locate outliers, never to certify naturalness.
State exactly which intervals were heard. If listening is unavailable, label the
review instrumented and leave naturalness/identity judgment pending for user
audition; ASR correctness cannot substitute for a perceptual pass.

Use at most three coordinated review/fix rounds per authorized narration
experiment/asset, stopping at the first clear round. The voice role shares the
existing cap; changing reviewers or retakes does not reset it. Report material
unresolved cadence/identity issues at the limit. For a short continuity experiment,
regenerate the complete bounded take when speech needs repair. For longer work,
test any context-aware passage repair with both neighboring joins instead of
assuming a splice will preserve identity. After an authorized repair, reconcile
fresh cues to canonical words before locking final visuals and retain uncertainty.

## Tested setup and limits

The production record verified the following on an RTX 4080 SUPER with 16 GB
VRAM. These are recorded observations, not benchmarks run by this skill:

| Item | Verified configuration or observation |
| --- | --- |
| Runtime | Python 3.12.10, PyTorch 2.9.1+cu128, Transformers 5.0.0 |
| Model and codec | MOSS-TTS-Local-Transformer-v1.5 and MOSS-Audio-Tokenizer-v2 |
| Codec precision | BF16 encoder/decoder with FP32 quantizer |
| GPU loading | Staged; 7.98 GiB allocated, 8.15 GiB reserved, maximum observed system snapshot 10,238 MiB |
| Generation plus decode | 1.50-1.69 seconds per audio second, excluding model and reference loading/preparation |

These GPU observations do not prove simultaneous residency of every component
or headroom beside another workload. Reuse staged loading and measure the new
workload before claiming fit. Timing excludes startup and does not establish
end-to-end latency or arbitrary-text throughput. Qwen was not re-benchmarked,
so these numbers do not establish a speed win.

The official [MOSS model card](https://huggingface.co/OpenMOSS-Team/MOSS-TTS-Local-Transformer-v1.5)
and [codec card](https://huggingface.co/OpenMOSS-Team/MOSS-Audio-Tokenizer-v2)
identify Apache 2.0 licenses, checked on 2026-10-12. Pin exact revisions and retain
applicable notices when redistributing; this does not audit every dependency or
reference asset. Follow the pinned implementation for loading and dtype handling
rather than casting every codec component to one precision.

## Configurable storage and recoverable migration

Resolve the model root from the user's existing configuration. Reuse the chosen
large-capacity volume for weights, runtimes, and scoped caches instead of filling
the system drive. Record configurable runtime, MOSS weight, codec, Qwen Base and
VoiceDesign, and Hugging Face cache paths. Example relative folders beneath that
root are `.runtimes/MOSS-TTS-v1.5`, `MOSS-TTS-Local-Transformer-v1.5`, and
`MOSS-Audio-Tokenizer-v2`; they do not prescribe a drive or automatic migration.

For an authorized move, retain rollback material, verify hashes and inference
from new paths, and test old-path junctions or compatibility links. Scope cache
changes to the relevant process/runtime; do not change global settings merely
to relocate one model. The recorded Qwen Base/VoiceDesign and scoped-cache move
passed new-path and legacy-junction tests with rollback. A directory move alone
is not a functioning inference test.

## Sample and alignment evidence

Compare voices with the same canonical text, synthetic reference, scene,
playback device, and matched encoded loudness. Retain raw WAVs, settings, and
actual listening preference separately from ASR/signal results. The preferred
engine still needs the lesson's representative voiced sample and bounded reviews.

Build word highlighting, scene windows, and visual anchors from the actual final
waveform. Reconcile ASR/DTW estimates to canonical words, retain recognition
differences and uncertain boundaries, and require listening where ambiguity
matters. Canonical-text alignment can estimate location without proving a word
was spoken. Preserve approved intro/body clips and exact sample offsets through
assembly, then verify encoded loudness, true peak, and sync under
[audio and delivery](audio-and-delivery.md).
