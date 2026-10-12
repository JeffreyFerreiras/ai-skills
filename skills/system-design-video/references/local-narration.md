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
