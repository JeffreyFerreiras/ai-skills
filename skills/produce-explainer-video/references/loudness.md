# Encoded loudness and sync

Measure **both final master and final phone encode**. PCM sample peak or a clean decode alone can miss quiet speech and inter-sample/AAC peaks. Record integrated LUFS, true peak dBTP, channel layout, measurement settings and exact encoded checksum. A quieter program with one near-ceiling transient cannot safely take an arbitrary gain increase.

For speech on mobile, start near -18 LUFS, with a review range around -20 to -16 LUFS and a final decoded ceiling at or below -2 dBTP. These are conservative production choices, not universal platform requirements. Adjust to the user's destination, channel layout and actual listening; prioritize headroom over hitting a loudness number. Target about -3 dBTP before lossy encoding to leave extra margin, then measure the encoded result. Mono and stereo measurement conventions differ; these recipes consistently use `dual_mono=false`. Document a changed convention and do not compare incompatible measurements.

## Measure, normalize, remeasure

Run the read-only helper independently for each final file:

```
python scripts/loudness.py WORK/master.mp4
python scripts/loudness.py WORK/phone.mp4
```

It prints decoded-input measurements, hash, pending listening scope and a measured second-pass filter, failing the selected range/ceiling gate when needed. Override `--min-lufs`, `--max-lufs`, `--max-dbtp` for an explicit delivery contract. The two passes share a target at the selected range midpoint and a pre-encode peak with extra margin; `--target-lufs`/`--target-dbtp` select other compatible targets. Unsupported/incompatible targets fail before processing. Non-finite/silent measurements fail closed; review short or deliberately silent material separately. The helper analyses the first audio stream across its complete runtime; inspect any additional delivery tracks separately.

For a quiet export, measure the exact preserved production mix first with the same target/settings. Use all five measured values in the printed `second_pass_filter` to run a second normalization pass. Supply that filter as `FILTER` below, with actual values rather than placeholders:

```
ffmpeg -nostdin -n -i WORK/master.mp4 -map 0:v:0 -map 0:a:0 -c:v copy -af FILTER -ar 48000 -c:a aac -b:a 128k -movflags +faststart WORK/master-loudness-r2.mp4
```

The second pass must process the same input measured in pass one. Prefer the preserved lossless mix when remuxing a production video: measure that mix, use a separate audio input with `-map 1:a:0`, and retain its timeline origin. Preserve native WAV/cues unchanged; save derived mixes and recipes as new revisions. No resynthesis, speed/rate change, trim, padding or default delay adjustment. Explicit output resampling sets the delivery sample rate without changing playback speed. FFmpeg may fall back from linear to dynamic normalization when peaks or loudness range prevent linear scaling; inspect its reported mode and review dynamics by ear. See [FFmpeg loudnorm documentation](https://ffmpeg.org/ffmpeg-filters.html#loudnorm).

AAC can overshoot the pre-encode ceiling. Remeasure **every final encoded revision**, including a phone transcode of an accepted master. If it overshoots, reduce the pre-encode target or apply a measured negative gain to the preserved normalized mix and re-encode to a new file. Start attenuation from `ceiling - measured_true_peak` with extra margin, then measure again; do not assume one correction guarantees the result. Recheck loudness as attenuation can put it outside the chosen range. Prefer a modestly quieter result to clipping or excessive dynamics processing. Never keep adding gain just to reach a LUFS target.

## Final evidence

After any audio repair, full-decode both outputs; compare video bitstream hash, frame counts/timestamps and duration with the accepted cut. For audio-only repairs stream-copy video. Check audio start/duration and waveform alignment over several distributed voiced windows (including early/late windows); record correlation lag, method and tolerance, not just file lengths. Correct encoding delay only from measured evidence. Recheck source WAV hash and voice/rate/caption sync. Phone compression/size QA must still pass after repair.

Have an independent reviewer inspect final encoded measurements and sync evidence. Separately listen at realistic phone volume for intelligibility, transients, pumping, beeps and caption sync. Measurement QA, sampled waveform alignment, independent forced alignment and actual listening are distinct evidence; none implies the others. Do not mark audio user-ready when listening is unavailable.
