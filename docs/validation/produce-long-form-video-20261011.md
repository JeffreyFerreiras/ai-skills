# Long-form video production validation, 2026-10-11

Scope: PR #32, `produce-long-form-video`. PR #31 remains the separate vertical video skill. These are local changes; no commit, push, profile synchronization or publication was performed.

## Implemented

- A horizontal 16:9 teaching workflow with progressive chapter questions, visible values and consequences, bounded independent technical/editorial and scene-diva reviews.
- Editable comparison and proportional timeline views, plus browser, copy-stack, database, price-tag and receipt surfaces. Clean cuts retain the board; stored values remain visible when a component is muted.
- Caption timing, groups, line wraps and glyph positions compile once per unchanged cue/font/layout input. Frame lookup uses binary search. Frozen source hashes remain authoritative.
- Each frame is drawn and geometrically audited once into bounded lossless RGB H.264 batches. Preflight saves reusable batches; resume binds source, pipeline, range, size, FPS, checked count and encoded hash. Delivery encoding still follows decode/timestamp/join checks.

## Executed checks

`python -m unittest discover -s skills/produce-long-form-video/tests -v`: 20 passed with Pillow, FFmpeg/ffprobe 9.0.2 and `VIDEO_TEST_FONT` pointing to local Arial. No skips in this run. Tests cover caption reuse/invalidation, timed reveals, unchanged source pixels after lossless encoding, 61 draws for 61 frames across preflight/resume, source mutation, and a later-batch geometry fault that cannot produce a receipt or final output.

`python scripts/build-docs.py --check`: passed, catalog and local links resolve.

`python SKILL_CREATOR/scripts/quick_validate.py skills/produce-long-form-video`: passed.

`python skills/skill-doctor/scripts/skill_doctor.py ISOLATED_PACKAGE --strict`: 0 errors, 0 warnings. The repository-wide six pre-existing graph resource errors are outside this package and were not changed.

`git diff --check`: passed, with only Git's README line-ending notice. Earlier root tests remain applicable; no root runtime behavior changed in this pass.

## Real production exercise

Created an original cache-freshness chapter with 415 cue words and 156.8 seconds of native Kokoro `af_heart`, speed 1.0, at 1920x1080 and 24 fps. Primary factual reference: [Microsoft Cache-Aside](https://learn.microsoft.com/en-us/azure/architecture/patterns/cache-aside). Product 42, prices 40/45, the ten-second clock and checkout policy are illustrative choices.

Draft review caught and repaired a missing rewind between alternative freshness policies. Visual review caught generic cards, a misleading direct database-to-cache arrow, and dark scene resets. Repairs use recognizable roles, application-mediated copy transfers and clean cuts. Review verdicts retain their actual inspection scope; frame sampling is not continuous watching or listening.

Encoded technical review also caught a reply leaving before its request arrived and a returned-value readout appearing before reply arrival. The repaired event table ends the request at 28.425s before its response proceeds, and shows the returned value at 42.100s on arrival. The skill now explicitly requires these dependency checks and measured sample loudness before delivery. Inspection of actual production found defects that template unit tests could not establish.

Local artifact directory: `%USERPROFILE%\Documents\Codex\video-production-validation\cache-freshness`. Its delivery report records final files, hashes, encoded QA, loudness, review limits and recovery checks. Media, generated art, voice files and model weights are not committed to this repository. The recovery ZIP includes editable scene code/configuration, exact native WAV, cues, script, tool sources and dependency versions.

Caption-plan benchmark on the 415-word chapter: mean cold compile 74.26 ms, cached access 0.622 ms. This measures plan access only, not total render speed. A 120-frame codec check encoded in 1.19 seconds and verified identical decoded RGB hashes. These are local observations, not cross-machine performance guarantees.

The initial sample measured -25.53 LUFS. Its corrected delivery copy measured -18.02 LUFS and -2.98 dBTP. Three distributed waveform checks found zero lag, with correlation above 0.99; video frames were unchanged. This establishes level/timing evidence, not pronunciation or listening approval. Continuous watch/listen remains a separate open gate until actual feedback is recorded.
