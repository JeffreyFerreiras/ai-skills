# Audio and delivery

Use the destination's audio/container requirements and the user's chosen route.
Preserve original narration, voice/rate, cues, and the accepted continuous master.
Record measurements and delivery evidence in the current production record.

## Measure the encoded audio

Measure integrated loudness in LUFS and true peak in dBTP on the final encoded
master and every additional encode actually produced. Do not create an optional
copy solely for QA. PCM sample peaks can miss AAC and inter-sample overshoots.
Record encoded hash, measurement command/settings, channel layout, and targets.

Without a destination contract, a conservative speech starting point is -18 LUFS,
with a review range of -20 to -16 LUFS and final true peak at or below -2 dBTP.
Aim near -3 dBTP before lossy encoding for headroom. These are production choices,
not universal platform limits. Adjust from destination requirements and actual
listening; do not sacrifice headroom to hit a loudness number. Record the channel
convention; the following recipe uses `dual_mono=false`:

```bash
ffmpeg -hide_banner -nostdin -i outputs/explainer.mp4 -map 0:a:0 -vn -sn -dn -af loudnorm=I=-18:TP=-3:LRA=11:dual_mono=false:print_format=json -f null -
```

Use the reported input measurements to assess this encoded file, not the filter's
normalized output measurements. Check every delivery audio track separately.
Silence/non-finite results do not pass a narrated-video check. Deliberately silent
projects need no narration measurement. If speech sounds quiet, also inspect
speech-bearing intervals; full-program values can conceal quiet dialogue.

## Repair without changing the performance

For normalization, measure the exact input that the second pass will process,
preferably the preserved lossless production mix. Carry `input_i`, `input_tp`,
`input_lra`, `input_thresh`, and `target_offset` into the second-pass `loudnorm`
fields `measured_I`, `measured_TP`, `measured_LRA`, `measured_thresh`, and `offset`.
Use the same I/TP/LRA targets and channel convention in both passes. See the
[FFmpeg loudnorm contract](https://ffmpeg.org/ffmpeg-filters.html#loudnorm).
Do not measure the MP4 and apply those values to a different WAV.

Write a new output, stream-copy video for an audio-only repair, and retain the
audio timeline origin. No default trim, time stretch, resynthesis, or delay
adjustment. A delivery sample-rate conversion must preserve playback speed.
Inspect whether FFmpeg used linear or dynamic normalization and listen for
pumping. Never use blind gain when peaks already constrain headroom.

Remeasure the actual encoded result after every repair. If AAC overshoots the
ceiling, lower the pre-encode target or apply measured attenuation to the preserved
normalized mix, then encode and measure again. Recheck loudness too. Prefer a
modestly quieter result to clipping; do not repeatedly amplify it to chase LUFS.

Full-decode repaired outputs. Compare video bitstream hashes, frame counts and
timestamps with the accepted cut. Verify audio start/end, original WAV hashes,
and waveform alignment in distributed voiced windows including early and late
speech. Record method, measured lag, and tolerance; equal file lengths do not
prove sync. Recheck caption alignment and listen at a realistic destination volume.
Measurements, waveform checks, and actual listening remain separate evidence.
When listening is unavailable, leave that requirement pending.

## Diagnose quiet destination playback

Before another full render, verify which bytes/version were delivered. Separate
the local master, uploaded file, and any destination transcode. Record when
downloaded playback bytes are inaccessible; do not invent a root cause.

Compare a short sample of the same audio as WAV and inside video on the same
device/app at fixed volume. Check codec/profile, sample rate, channel layout,
per-channel levels, metadata, and polarity. A controlled compatibility candidate
may use H.264, AAC-LC at 48 kHz, clean nonessential metadata, and fast-start MP4.
Change one variable at a time and obtain actual listening feedback before a full
compatibility rebuild. Sample feedback covers that configuration, not the episode.

If testing mono-to-stereo duplication, use explicit unity channel mapping such
as `pan=stereo|c0=c0|c1=c0` for a mono source and verify levels/sync. Automatic
channel conversion may use different gains. Equal unity channels can raise
aggregate LUFS by about 3 LU with `dual_mono=false` without increasing either
channel's gain. Do not attenuate approved playback just to match its former mono
LUFS value. Mono is not universally broken, and stereo is not a guaranteed fix.

## Deliver one continuous final

Deliver the complete continuous video at the reviewed quality. An already
accepted continuous master needs identity/completeness verification, not another
stitch or encode. A local-only request is satisfied through the requested local
handoff; it does not authorize an upload. For a transfer request, only a verified
successful whole-file transfer establishes delivery. Record the actual receipt
and file identity evidence. Tool errors, invented URLs, and preview receipts do
not establish final delivery.

Resolve the actual destination/tool byte limit. If the master exceeds it, use an
authorized private route that supports the whole file or ask for the specific
destination needed. Keep the master unchanged and delivery pending until that
route succeeds. Do not silently lower quality, publish a private file, or replace
the final with chapter clips. Route changes alone do not call for reprocessing.

Create phone copies or chapter previews only when requested. Keep them distinct
from the continuous full-quality final. For a size-limited copy, make bounded
attempts against an explicit byte budget and inspect encoded sharpness, captions,
diagrams, and motion. A bitrate calculation or familiar width is not approval.
If quality cannot fit, report the limit and return to whole-file delivery.

When only video size must change and approved AAC already works, stream-copy
audio with `-c:a copy`. Verify packet payload hashes, timestamps, codec identity,
and priming/skip data before claiming preservation. Remeasure final audio even
after copy; if audio is re-encoded, renew listening and sync checks too. Cutting
chapter clips can change AAC priming/timestamps, so whole-stream equivalence
cannot be assumed. Map local/global timecodes and inspect each clip's boundaries.

Delivery, full acceptance, and publication are separate. Acceptance needs explicit
full-video feedback or a satisfied user-defined condition. Publication needs its
own authorization and verified result. Keep each status tied to the exact file.
