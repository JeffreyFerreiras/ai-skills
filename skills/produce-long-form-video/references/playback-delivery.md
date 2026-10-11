# Playback compatibility and continuous full-file delivery

## Quiet playback despite acceptable measurements

Use a short diagnostic ladder before another full render. Passing LUFS/true-peak checks does not prove the destination app/device will play the file as expected. Preserve the original master and approved native voice/rate/cues throughout.

1. Verify the **delivered file identity**: encoded checksum, version, attachment success and, when accessible, downloaded playback bytes. Distinguish the local export, uploaded artifact and app playback/transcode. Inspect codec/profile/sample rate/channel layout, metadata/gain tags and channel polarity, but do not claim a root cause without evidence. If app playback bytes are inaccessible, say so.
2. Measure speech-bearing portions as well as full-program loudness; silence/music/transients can conceal quiet narration. Compare per-channel RMS/peaks and waveform sync. Do not keep raising gain when peaks already constrain headroom. Remeasure any repaired encode as specified in [loudness QA](loudness.md).
3. Compare the **same audio** as WAV and inside video on the same device/app at a fixed volume. Decode the video audio when needed to verify that an A/B pair actually shares its waveform. Use one clearly labeled controlled sample to avoid version confusion. A WAV that sounds good while video stays quiet points toward a playback/container/codec route worth testing; it does not prove which component is responsible.
4. Make a short compatibility sample with the same narration and unchanged per-channel level. A useful candidate is fresh H.264 video, AAC-LC 48 kHz/256 kbps, clean nonessential metadata and faststart. For mono narration, explicitly duplicate at unity to two channels, then verify channels/peaks/sync and get **actual user listening feedback** before rebuilding the full episode. Mono is not universally broken, and stereo does not guarantee a fix. Avoid changing several acoustic variables or repeating full encodes without this gate.
5. Record the exact selected sample/audio settings and feedback. A compatibility-sample approval covers that sample/configuration, not the complete video, visual quality, delivery or publication. Preserve sample-first production and final full playback acceptance as separate gates.

For a preselected mono WAV, `pan=stereo|c0=c0|c1=c0` duplicates samples at unity; automatic `-ac 2` conversion can use other gains. A controlled video sample can map its audio input with `-map 1:a:0`, use that pan filter, `-ar 48000 -c:a aac -profile:a aac_low -b:a 256k`, and `-map_metadata -1 -map_chapters -1 -movflags +faststart`. Use measured matching sample boundaries and fresh outputs; no default trim, stretch or offset repair. See [FFmpeg pan](https://ffmpeg.org/ffmpeg-filters.html#pan-1) and [stream selection/copy](https://ffmpeg.org/ffmpeg.html#Streamcopy).

With `dual_mono=false`, duplicating equal mono at unity can raise two-channel integrated loudness by about 3 LU because both channels contribute energy. This is **not** a 3 dB gain to each channel. Record per-channel peaks/RMS and channel count; compare compatible measurement conventions. Do not attenuate an approved stereo compatibility sample solely to make its aggregate LUFS match the former mono value. Recheck safe peaks and document a suitable delivery target.

## One continuous full-quality final

Deliver **one continuous full-quality final video**. If the continuous master already exists, verify its identity/completeness and keep it unchanged; do not re-stitch or re-encode an already completed master merely to satisfy a request for the whole video. Approved visual quality, audio packets/timing and complete narration must survive delivery.

Resolve the actual destination/tool byte limit rather than inventing a universal chat limit. If chat cannot attach the approved full-quality file, use a supported authorized **private full-file** delivery route that can hold it, or ask for the specific destination needed. Check route capability/access and report any remaining blocker. Do not invent a share/download URL; use only an actual verified tool result, and do not publish or expose a private file without authorization. Confirm successful full-file transfer and file identity. A tool error, rejected oversized file or chapter-preview send does not establish final delivery.

Do not downscale/compress away approved quality just to make the final attach. Chapter clips may be optional requested previews with clear labels and boundary/caption/audio QA; they are never an unsolicited substitute for the continuous final file, nor full acceptance. Preserve full-file delivery as pending until the supported route succeeds. No new episode processing is implied by a delivery-route change.

## Optional reviewed phone copy

When an additional phone-size/compatibility copy is requested, retain the full-quality final. If approved AAC already works, a separate copy can reduce **only video**, within reviewed quality and an actual byte budget:

```
python scripts/phone_copy.py WORK/approved-master.mp4 WORK/phone-r2.mp4 --copy-audio --max-bytes N --width 540
```

This route uses `-c:a copy`, budgets from actual AAC packet sizes, and requires exact packet payload hashes, timestamps, priming/skip data and codec identity to match. It leaves the master untouched and fails before promoting a file if audio changes, the actual size exceeds budget or decode/timing QA fails. The ordinary phone route re-encodes audio and therefore needs renewed listening; use copy mode when audio preservation is the requirement.

If the budget cannot retain approved audio and visual quality, stop and report the limit; return to the private whole-file route rather than degrading the final or substituting parts. Make bounded optional-copy attempts with explicit budgets, checking actual size and caption/diagram readability each time. Preserve frame timing and narration duration. Recheck final audio measurements, destination playback and successful delivery receipt. Never mark full user-ready or published from an upload attempt or sample feedback alone.

Treat a size-constrained encode as a **new visual candidate**, not an automatic accepted copy. Inspect encoded asset sharpness, contours, crop detail, captions/diagrams and motion against the approved visuals. The width in the example and any calculated bitrate are inputs to review, not preferred quality defaults; low bitrate at a familiar phone width can still look soft. Passing byte/decode checks proves neither attractiveness nor legibility. Keep approved audio unchanged during visual-quality repairs.

Explain route/access or optional-copy tradeoffs without changing the one-file final requirement. For explicitly requested chapter previews, map global/local timecodes and check meaningful boundaries, captions, continuity and audio coverage/decoded waveform; AAC cutting can alter priming/timestamps and is not automatically equivalent to whole-stream copy. Review each preview within its cap, label its scope and keep the continuous master untouched. Confirm destination support and actual receipts rather than treating previews or failed transfer attempts as final delivery.
