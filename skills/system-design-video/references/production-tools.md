# Production tools

Use these helpers for a new Remotion project or repeated production checks. Reuse
an existing project's renderer and commands when available. The Python helpers
need Python 3.10+ and the standard library. They never install packages, choose a
paid provider, publish, or grant review approval.

Commands below run from this skill's directory. `VIDEO_PROJECT` means the user's
chosen project outside the skill. Use the host shell's environment-variable syntax
and quote paths. In CMD, substitute `%VIDEO_PROJECT%` for `$VIDEO_PROJECT`.

## New project and capability check

```bash
python scripts/video_project.py init "$VIDEO_PROJECT"
python scripts/video_project.py preflight "$VIDEO_PROJECT/video-spec.json"
```

Init requires a nonexistent destination. It copies the
[Remotion starter package](../assets/remotion-starter/package.json) and
[v2 fixture plan](../assets/reference-video.json). Adapt the question, sources,
script, style, format and diagram before using it for a real lesson. The fixture
uses 360x640 for cheap testing, not the production default. The starter supports
integer frame rates and a simple vertical layout. Supply node `position` values
in its 1080x1920 SVG coordinates or customize scene code for denser diagrams.
It is a starting composition, not a universal visual design.

In the project, run `npm ci` only within existing installation authority;
the starter includes a lockfile. `npm run studio`, `npm run render`, and
`npm run still -- outputs/frame.png --frame=30` use the pinned renderer.
The first render can require a browser download; prefer an available compatible
browser through Remotion's `--browser-executable` option when appropriate.
Observe the renderer's license for the actual use. The starter uses system Arial
with no bundled fonts or external artwork; pin or bundle authorized fonts when
cross-host pixel identity matters.

Place measured WAVs under `public/audio` and retain that relative path, including
`public/`, in the spec. The starter rejects planned audio at render time. It draws
packets over their duration, applies state labels on completion, and derives all
frames from timeline position. Set `presentation.burn_captions` only when the brief
requires burned subtitles. Otherwise scene teaching captions remain visible and
the aligned subtitle track is delivered separately.

Preflight probes Node, FFmpeg, FFprobe and renderer presence. Pass `--ffmpeg` and
`--ffprobe` to use configured executable paths. Package presence is not a render
test. Supply `--capabilities capabilities.json` for host-dependent evidence:

```json
{
  "independent_review": {"status": "unknown", "evidence": ""},
  "playback": {"status": "unknown", "evidence": ""},
  "hardware": {"status": "unknown", "evidence": ""},
  "voice": {"status": "unknown", "evidence": ""},
  "alignment": {"status": "unknown", "evidence": ""},
  "listening": {"status": "unknown", "evidence": ""}
}
```

Replace a status with `available` only with concrete evidence: permitted reviewer
dispatch and selected assignments, actual playback/listening support, measured
GPU/RAM headroom and disk needs, a working selected voice, and the chosen alignment
precision. A present executable, ASR transcript or model self-report does not prove
perceptual listening. Silent work omits voice/alignment/listening checks. Preflight
returns exit 1 for unresolved items and reports disk space; it never clears a
production gate or prevents independent plan work.

## Previews and encoded checks

```bash
python scripts/video_project.py previews "$VIDEO_PROJECT/video-spec.json"
python scripts/video_project.py media-check "$VIDEO_PROJECT/video-spec.json" "$VIDEO_PROJECT/outputs/explainer.mp4" --bridge-record "$VIDEO_PROJECT/scene-bridge-encoded.json"
```

Follow the [story bridge contract](story-bridge-contract.md) before media-check.
It requires actual implementation, encoded samples and distinct independent
story verdicts, including blind interpretation before intent. The media-check
CLI rejects a missing record or mismatched story/timeline/video evidence. Direct
`check_media` calls used by technical fixtures remain technical checks only.

`previews` returns deduplicated frame indexes around starts, midpoints, completions,
and scene boundaries. Render the relevant indexes with the project's still command;
inspect moving intervals separately. `media-check` probes dimensions, frame rate,
required audio, stream start/end within 0.1 seconds, full decode, and each encoded
audio track's integrated loudness and true peak. It defaults to the documented
-2 dBTP ceiling, reports measurements and commands, and leaves visual quality,
listening and cue/content alignment pending. Destination-specific targets and
declared end holds need the project's own checks. It does not normalize audio or
claim that a matching duration proves synchronization.

## Artifact manifests and durable receipts

```bash
python scripts/video_project.py snapshot "$VIDEO_PROJECT" --path src --path video-spec.json --path public --path package-lock.json --plan "$VIDEO_PROJECT/frozen-plan.json"
python scripts/video_project.py record --state "$VIDEO_PROJECT/review-state.json" --manifest "$VIDEO_PROJECT/candidate.json" --entry "$VIDEO_PROJECT/review-entry.json"
```

Save snapshot JSON as `candidate.json` outside the selected paths. Enumerate every
input and output required for the gate, including source, audio/cues, assets,
dependency locks and the exact rendered candidate. The frozen plan records stage,
criteria, workspace/base revision, exclusions, assignments and checks. The helper
hashes actual file bytes and a canonical JSON manifest, includes file modes and
empty directories, and rejects linked paths and embedded Git metadata before
descending into each directory. Windows reparse-point detection uses
filesystem attributes available on Python 3.10+, including directory junctions.
It does not infer scope, follow symlinks, or implement submodule snapshots. For a later
candidate, `--baseline candidate.json` represents removed files with their prior
identity. Treat renames as explicit old-path deletion plus new-path addition and
record the relationship in the plan.

`record` requires an entry `kind` (`dispatch`, `validation`, `review`, `checkpoint`),
`stage`, and `identity`. Include the loop's attempt/round, selected model/effort,
command/exit, findings, decision and actual inspection coverage as applicable.
It checks the manifest against live artifacts, binds the entry to that digest,
then writes an integrity-checked sequence through atomic file replacement while
retaining `.previous`. A `.lock` rejects concurrent writers. After interruption,
reconcile the writer before removing a stale lock or `.tmp` file. State, backup,
lock and temporary destinations are all checked against candidate scope before
any write; a collision or stale temporary file leaves existing files unchanged.
Keep all state and entry files outside candidate scope. The previous checkpoint
is recovery data, not permission
to rerun an uncertain worker.

These helpers automate artifact identity and receipt persistence. The Supervisor
still reserves the shared three-round budget, validates reviewer identities,
reconciles dispatch, checks criteria and authorizes transitions under the
[review loop](video-review-loop.md). An arbitrary recorded `APPROVE` is not a gate
pass. Do not claim full orchestration or host isolation from these local helpers.

## Reproducible integration fixture

Use [reference_video.py](../scripts/reference_video.py) in a new scratch project.
On Windows, the explicit `--synthesize-windows` option uses local System.Speech
for synthetic test narration, without changing the production voice preference:

```bash
python scripts/reference_video.py prepare "$VIDEO_PROJECT" --synthesize-windows
```

On another host, use `--voice-dir` pointing to authorized PCM clips `send.wav` and
`receive.wav` containing the two fixture sentences. Preparation records voice
provenance, measures both clips, reports the deliberately short draft's fit errors,
extends scene windows and dependent events, and emits `subtitles.srt`. Original
speech samples remain unchanged; captions use scene-level clip timing, not word
alignment. After installing the starter dependencies in that project:

```bash
python scripts/reference_video.py render "$VIDEO_PROJECT"
```

Pass configured `--ffmpeg`, `--ffprobe` and optional `--browser` paths. Rendering
requires a fresh `outputs` directory and writes one continuous MP4, representative
stills decoded from that MP4 and `media-report.json`. Use the `inspect` subcommand
with the same media-tool paths to recheck an existing export without rerendering;
sample filenames include the encoded file hash. Retain the project lockfile, commands and reports
as reproducibility evidence. The fixture tests integration and retiming, not
production voice quality, viewer comprehension or independent approval. Use the
real lesson's review gates for those judgments.
