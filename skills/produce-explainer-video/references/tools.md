# Local tools and contracts

## Choose the available route

Check Python/Pillow, FFmpeg/ffprobe, a licensed local font, and the selected voice runtime. Check Node/resvg only for SVG production; Three.js only for real geometry. Run `python scripts/capabilities.py` from this skill to report capability gaps. It never installs software or downloads weights.

The observed polished route was SVG/resvg plus alpha-preserving PNG/Pillow compositing and FFmpeg. Characters were pose crossfades. Earlier real geometry used Three.js and official SVGRenderer in software, rasterized with resvg; that was not WebGL/GPU rendering. Either is optional. NotebookLM, cloud connectors, Remotion and paid renderers are not prerequisites.

The portable Pillow starter is a small, editable layered renderer, not a replacement for concept-specific art direction. Copy [scene.py](../assets/scene.py) and [project.json](../assets/project.json) into a new source project outside this skill. Create narration, audio, cues, font/asset provenance and authored scenes there. The `scenes` array is intentionally empty. Define start/end from measured speech, type (`context` or `diagram`), title, nodes (`id`, `label`, `box`, optional `reveal`/`activate`) and paths (`id`, `points`, local `start`/`end`). Positions use a 540×960 design canvas. Output dimensions and caption safe box are configurable; the caption box uses output pixels. More complex compositions should implement the same frame interface.

The episode renderer exports `frame(time_seconds, config, root)` returning a Pillow image and transformed geometry objects with `id`, `kind`, `box` and optional `alpha`. Include node bodies/shadows, labels, actors, static path segments/arrowheads, moving pulses and complete halo envelopes. For other renderers export equivalent geometry for every frame. Rectangular audits are conservative and do not prove alpha silhouettes, path topology or intentional docking safe. Visually inspect whole paths, crossfades, layers, crop and phone captions; do not hide an issue by changing its type. Use path `reveal` and endpoint `from`/`to` node IDs so connectors do not precede required endpoint reveals. Explicitly display data mutations and intermediate values when they are taught.

## Native audio and captions

`python scripts/audio.py generate PROJECT/narration.txt --output PROJECT/audio` uses Kokoro `af_heart` speed 1.0 on CPU. Install missing dependencies in a project-local environment if authorized; pin the actual versions afterward. The working source used Kokoro 0.9.4, but use a compatible installed runtime rather than inventing version pins. Weights and caches stay outside packaged sources. Explicit voice/speed choices override defaults.

`python scripts/audio.py audit PROJECT/audio/narration.wav` measures PCM16 duration, peak/clipping and checksum. `python scripts/audio.py cues PROJECT/audio/words.json --script PROJECT/narration.txt --wav PROJECT/audio/narration.wav` checks exact text and ordered cue coverage. The generated word times are model estimates, not independent forced alignment. Inspect pronunciation and highlight sync with playback. Preserve the exact original WAV; resynthesis changes timing and must produce a new revision. No artificial gaps, trim or default time stretch. Use [mix_events.py](../scripts/mix_events.py) for restrained optional event beeps in a separate mix, while retaining the native WAV and cue source.

The starter requires a font file inside the locked source root, measures its glyphs and produces higher screen-space karaoke captions with a two-line/crop guard. Groups respect sentences/paragraphs and measured scene starts, and remain visible during internal cue gaps; only the current word highlights. Anchor scene changes to spoken word starts; optional `caption_breaks` add boundaries. Deliver SRT via [export_srt.py](../scripts/export_srt.py) alongside the burned-in karaoke, using `--project PROJECT/project.json` to share scene boundaries. Sidecars need exact text and timing QA too.

## Freeze and recover

Keep all production inputs under one source root. Store all output/work directories, locks and checkpoints outside it. Include code, source passages or citations, script, exact WAV/cues, art, font license/provenance, timing/caption configuration and dependency lockfiles. Exclude caches, models and dependencies. Do not put private sources into the reusable skill repository.

```
python scripts/project.py lock PROJECT WORK/source-lock.json
python scripts/project.py check PROJECT WORK/source-lock.json
python scripts/project.py checkpoint PROJECT WORK/source-lock.json --output WORK/source.zip
```

The complete source file set is hashed. Additions, removals and changed bytes invalidate it. Checkpoints use deterministic entries and hash readback. Extract into a clean directory, read the reserved root `CHECKPOINT-MANIFEST.json`, use it as the external source lock, then run `project.py check` against the extracted project. That generated manifest is excluded from capture, so it can remain inside the restored root. Test reproduction from preserved WAV without synthesizing again. A reviewed lock is never overwritten; use a new revision after repair.

## Preflight, render and resume

```
python scripts/render.py PROJECT --lock WORK/source-lock.json --output WORK/sample.mp4 --seconds 25 --preflight-only
python scripts/render.py PROJECT --lock WORK/source-lock.json --output WORK/sample.mp4 --seconds 25
```

Select a representative sample range with `--start`/`--seconds`, or make a separate sample project for noncontiguous scenes. A discontinuous montage is not continuous full playback. The sample must include core teaching, a transition, captions and native narration, not merely pass a command. Export intermediate PNGs through the renderer's frame function when visual review needs them.

Before full production create an evidence receipt outside the source root, containing `source_set_sha256` and three objects: `sample_approved`, `technical_preflight`, `editorial_animatic`, each with `passed: true` and a concrete `evidence` string naming feedback/review scope. Author it only from actual completed gates. Receipt validation prevents stale approvals but cannot verify reviewer independence or user consent. Do not fabricate receipts to unlock rendering.

```
python scripts/render.py PROJECT --lock WORK/source-lock.json --output WORK/master.mp4 --mode full --review WORK/review.json --resume
```

The renderer checks sources before/after bounded lossless batches, verifies decoded batch joins, frame count and frame timestamps, and promotes a unique partial file only after complete encoded QA. Time is `start + index/fps`; full frame count is `ceil(WAV seconds * fps)`. Keep the sub-frame visual tail rather than alter speech. Source PCM is retained even though delivery AAC is lossy. Existing finals are never overwritten. Resume receipts bind source set, range, dimensions and FPS and verify batch hashes. A project/output lock prevents concurrent pipelines on shared inputs; a stale lock requires verifying its process ended before manually removing it. `--stop-file PATH` exits without declaring a partial complete. Already verified lossless batches can resume under the same lock. Run a new revision after any source change.

For layered SVG/PNG routes preserve each layer's alpha, clip, camera transform and z-order; world and screen overlays remain separate and the backdrop stays fixed. Cache keys include asset hash, opacity, transform and clipping. Validate cached and uncached raster outputs including alpha before reuse. Never reuse a layer ID for different asset bytes.

## Phone copy and final encoded QA

For reproducible sampled review, run `python scripts/review_frames.py VIDEO NEW_REVIEW_DIRECTORY --times 0.5,8.1,19.4`. It selects explicit decoded frame indices in one decode and records requested and actual encoded timestamps. Report the actual decoded frame time rather than labeling a requested seek time as exact. These extracted samples are still not continuous playback/listening.

Create a separate delivery copy using the master as input, with target dimensions and a measured size budget. `python scripts/phone_copy.py MASTER PHONE --max-bytes N --width 540` uses two-pass H.264/AAC, checks actual size, timestamps and full decode, and keeps the master untouched. Size is a delivery requirement, not a universal platform limit. Phone legibility and audio listening remain separate reviews. Do not claim lower resolution is visually equivalent without evidence.

The helpers adapt the working source's source locks, deterministic checkpoint, bounded renderer, caption checks, transformed geometry and FFmpeg gates. They remove hardcoded episode paths and Linux-only `fcntl`. They do not bundle artwork, audio, models or historical review logs. Structural tests and a short encode verify mechanics; independent editorial review and user judgment verify the video within their stated scopes.
