# Transactional outbox reference pipeline

These scripts are the render, sound, pause, speech-to-text, encode QA, and loudness path from the transactional outbox master. They show the method. They are not the skill's supported spec checker, and they do not run from a clean checkout.

Scene names inside the renderer are production notes. The frame checks reject drawing them. The video that this pipeline builds is the clean master only.

## Expect these inputs beside the scripts

- `narration/fenrir-performance.json`, `narration/words.json`, and `narration/fenrir-performance.wav`. `timeline.py` reads the JSON. `build_audio.py` reads the WAV. The takes are one file per sentence, already assembled in that performance WAV.
- `lid-face.png` and `clean-plate.png` in an `assets` folder beside `chalkkit.py`. That module loads them at import. They are the physical laptop. They are not bundled.
- `CHALK_SOUND_HELPERS`: path to `build_sound.py` from the earlier thread-pool episode. `build_audio.py` refuses to run until that variable is set. The helper is not bundled.

The Jeffrit end card is imported from this skill's brand kit at [jeffrit](../../../assets/brand/jeffrit/README.md).

## Pieces

- `timeline.py`: word-timed scene boundaries. Internal scene names are never drawn.
- `chalkkit.py`: board, chalk marks, C# lexer tokens, limb-name check, 1080x1920 at 24 fps.
- `render.py` and `part2.py`: the picture. `part2.py` is the second half (fix, relay, trade-offs, laptop close, end card) and uses names from `render.py`.
- `events.json`: cue times written for the sound build.
- `build_audio.py`: narration, music high-passed at 230 Hz, SFX high-passed at 300 Hz, no whoosh in a pause, C add9 chord under the end card.
- `master.py`: iterative gain to -14 LUFS integrated (within 0.05) and true peak at or below -2 dBTP, then AAC in the MP4, measured with `ebur128`. The final master filename is written only after both checks pass. Ten misses keep the measurement report, leave the candidate off that filename, and exit nonzero.
- `measure_pauses.py`: sentence and paragraph gaps on the final MP4, plus words per minute. Pass the MP4, and optionally the performance JSON, as arguments.
- `asr_mp4.py`: `small.en` and `base.en`, beam 5 and greedy, per sentence and per paragraph of the final MP4. Every expected sentence is checked against the decoded duration. Missing or truncated sentence IDs are reported and the process exits nonzero. Complete sentences in a partly covered paragraph are still decoded.
- `enc_qa.py`: contact sheet, 360x640 frames, and a strip of every transition from the encoded file.

Do not point these scripts at a private machine path inside the skill. Pass the episode directory on the command line or through `CHALK_SOUND_HELPERS`.
