# Playback compatibility and bounded delivery

## Quiet playback despite acceptable measurements

Use a short diagnostic ladder before another full render. Passing LUFS/true-peak checks does not prove the destination app/device will play the file as expected. Preserve the original master and approved native voice/rate/cues throughout.

1. Verify the **delivered file identity**: encoded checksum, version, attachment success and, when accessible, downloaded playback bytes. Distinguish the local export, uploaded artifact and app playback/transcode. Inspect codec/profile/sample rate/channel layout, metadata/gain tags and channel polarity, but do not claim a root cause without evidence. If app playback bytes are inaccessible, say so.
2. Measure speech-bearing portions as well as full-program loudness; silence/music/transients can conceal quiet narration. Compare per-channel RMS/peaks and waveform sync. Do not keep raising gain when peaks already constrain headroom. Remeasure any repaired encode as specified in [loudness QA](loudness.md).
3. Compare the **same audio** as WAV and inside video on the same device/app at a fixed volume. Decode the video audio when needed to verify that an A/B pair actually shares its waveform. Use one clearly labeled controlled sample to avoid version confusion. A WAV that sounds good while video stays quiet points toward a playback/container/codec route worth testing; it does not prove which component is responsible.
4. Make a short compatibility sample with the same narration and unchanged per-channel level. A useful candidate is fresh H.264 video, AAC-LC 48 kHz/256 kbps, clean nonessential metadata and faststart. For mono narration, explicitly duplicate at unity to two channels, then verify channels/peaks/sync and get **actual user listening feedback** before rebuilding the full episode. Mono is not universally broken, and stereo does not guarantee a fix. Avoid changing several acoustic variables or repeating full encodes without this gate.
5. Record the exact selected sample/audio settings and feedback. A compatibility-sample approval covers that sample/configuration, not the complete video, visual quality, delivery or publication. Preserve sample-first production and final full playback acceptance as separate gates.

For a preselected mono WAV, `pan=stereo|c0=c0|c1=c0` duplicates samples at unity; automatic `-ac 2` conversion can use other gains. A controlled video sample can map its audio input with `-map 1:a:0`, use that pan filter, `-ar 48000 -c:a aac -profile:a aac_low -b:a 256k`, and `-map_metadata -1 -map_chapters -1 -movflags +faststart`. Use measured matching sample boundaries and fresh outputs; no default trim, stretch or offset repair. See [FFmpeg pan](https://ffmpeg.org/ffmpeg-filters.html#pan-1) and [stream selection/copy](https://ffmpeg.org/ffmpeg.html#Streamcopy).

With `dual_mono=false`, duplicating equal mono at unity can raise two-channel integrated loudness by about 3 LU because both channels contribute energy. This is **not** a 3 dB gain to each channel. Record per-channel peaks/RMS and channel count; compare compatible measurement conventions. Do not attenuate an approved stereo compatibility sample solely to make its aggregate LUFS match the former mono value. Recheck safe peaks and document a suitable delivery target.

## Attachment limits without losing approved audio

Keep the full-quality master. Resolve the actual destination/tool byte limit rather than inventing a universal chat limit; use a conservative budget below it. Confirm attachment/export success explicitly. A tool error or rejected oversized file is not accepted delivery, even if local rendering succeeded.

If approved AAC audio already works, reduce **only video** in a separate phone copy:

```
python scripts/phone_copy.py WORK/approved-master.mp4 WORK/phone-r2.mp4 --copy-audio --max-bytes N --width 540
```

This route uses `-c:a copy`, budgets from actual AAC packet sizes, and requires exact packet payload hashes, timestamps, priming/skip data and codec identity to match. It leaves the master untouched and fails before promoting a file if audio changes, the actual size exceeds budget or decode/timing QA fails. The ordinary phone route re-encodes audio and therefore needs renewed listening; use copy mode when audio preservation is the requirement.

If the budget cannot accommodate the approved audio plus useful video, stop and report the limit. Choose a lower reviewed video bitrate/resolution or another authorized delivery destination; do not silently degrade accepted audio. Make bounded attempts with explicit byte budgets, checking actual size each time. Inspect caption/diagram readability on the encoded phone copy before retrying upload. Preserve frame timing and narration duration; full-quality master delivery remains a separate artifact. Recheck final audio measurements, destination playback and delivery receipt. Never mark full user-ready or published from an upload attempt or sample feedback alone.
