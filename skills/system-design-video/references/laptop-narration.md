# Laptop narration with Kokoro and OpenVINO

Use for the new Intel Core Ultra 9 / Intel AI Boost, NVIDIA RTX 5070 laptop.
Kokoro-82M is a candidate for new local narration that leaves the NVIDIA GPU
available for video work. The exact laptop SKU, NPU generation, driver and local
performance remain unverified. "AI Boost" alone does not establish compatibility.
Prior severe stutter is user-reported; its cause has not been established.
Keep the existing [script gate and approved audio](local-narration.md) intact.
This path supplies setup and validation guidance plus a discovery helper, not a
bundled synthesis runner, weights, or a completed laptop benchmark.

## Verify the host before selecting NPU

On the intended laptop, record the exact CPU SKU, NPU hardware ID/generation,
Windows build, Intel NPU driver version, RAM and NVIDIA GPU/VRAM. Use read-only
System Information or Device Manager and compare against the selected release's
[system requirements](https://docs.openvino.ai/2026/about-openvino/release-notes-openvino/system-requirements.html)
and [NPU documentation](https://docs.openvino.ai/2026/openvino-workflow/running-inference/inference-devices-and-modes/npu-device.html).
The documented Windows NPU platform is Windows 11 x64, 22H2 or later, with a
separate supported driver. Do not infer generation from the Core Ultra 9 name,
update drivers, change security settings, or alter another device to pass preflight.
OpenVINO's `GPU` device targets Intel graphics; it does not mean the RTX 5070.

Run from this skill directory with the intended environment's Python:

```powershell
& $NarrationPython scripts/narration_probe.py --timeout 15
```

Save stdout to a new project evidence file. The helper discovers OpenVINO devices,
full device names, optional driver properties and package versions. Missing
properties remain errors for manual reconciliation. Exit 0 means discovery ran,
1 means the runtime/device query was unavailable, and 2 means a worker error or
deadline. No models are compiled or loaded. `execution_device: null`, pending NPU
validation and no CPU fallback execution are intentional even if NPU is listed.
This report alone cannot make the production preflight's `voice` or `hardware`
capability available. It inventories the host where it runs, not a remote laptop.

## Isolate and pin the candidate runtime

Within authorized setup, create a new Python environment under the configured
runtime/model root, separate from MOSS/Qwen and the system interpreter. A supported
Python 3.12 environment is a candidate; first verify official wheels for the exact
Windows/Python combination. Do not fall back to an unplanned source build.
Resolve model/cache roots from user configuration and scope cache/environment
variables to the worker. Do not change global PATH, execution policy or services.

Official [2026.4 release notes](https://docs.openvino.ai/2026/about-openvino/release-notes-openvino.html)
add Kokoro-82M NPU support. The later
[2026.4.1 runtime patch](https://github.com/openvinotoolkit/openvino/releases/tag/2026.4.1)
fixes Windows NPU crashes on Core Ultra Series 3 with specific drivers; that does
not identify this laptop or explain the reported stutter. Candidate published pins
checked on 2026-10-21 are `openvino==2026.4.1`,
[`openvino-genai==2026.4.1.0`](https://pypi.org/project/openvino-genai/2026.4.1.0/)
and [`openvino-tokenizers==2026.4.1.0`](https://pypi.org/project/openvino-tokenizers/2026.4.1.0/).
These are source-verified candidates, not a locally tested lockfile. Before use,
resolve their dependencies in the isolated environment, run `pip check`, record
the full resolved versions and retain a reproducible lock. Recheck support before
adopting a newer release. Do not copy nightly/prerelease installation commands
from an older model card into the production environment.

## Retain actual model and voice assets

Use the original [hexgrad/Kokoro-82M](https://huggingface.co/hexgrad/Kokoro-82M)
or an inspected, revision-pinned conversion such as
[OpenVINO/Kokoro-82M-int8-ov](https://huggingface.co/OpenVINO/Kokoro-82M-int8-ov).
The converted card labels it for evaluation; validate its files on the chosen
device rather than assuming all exports or quantizations work. Retain Apache 2.0
licenses/notices and inspect the exact model, voices, pronunciation assets and
dependencies for their terms. A model license alone does not audit the bundle.
Record source URLs, immutable revisions, local paths, hashes, export settings and
precision. Downloads or model export are separate authorized setup work.

The [official Kokoro sample](https://openvinotoolkit.github.io/openvino.genai/docs/samples/python/speech_generation/)
exports voice embeddings into `voices/*.bin`. Its export uses
`--trust-remote-code`; inspect the pinned implementation and applicable authority
before executing it. Do not run remote model code implicitly during preflight.
For English, unknown-word phonemization needs a verified local fallback.
`espeak-ng` is a separate GPLv3 dependency; the OpenVINO English G2P alternative
also requires local assets and uses CPU. Test technical terms and offline behavior.
NPU neural inference does not imply CPU-free preprocessing.

Audition the official US female voices `af_heart` and `af_bella` with identical
text and matched loudness, then select by listening. The
[voice inventory](https://huggingface.co/hexgrad/Kokoro-82M/blob/main/VOICES.md)
does not guarantee naturalness for this lesson. Retain the selected Kokoro voice
file and its hash; MOSS's synthetic reference and SpeechT5 embeddings are not
interchangeable with it. Load the float32 embedding to the shape returned by
[`get_speaker_embedding_shape()`](https://docs.openvino.ai/2026/api/genai_api/_autosummary/openvino_genai.Text2SpeechPipeline.html),
and reject absent or mismatched assets rather than changing voice silently.

## Run a bounded 10-second stability trial

After the script gate, use about 10 seconds of representative narration at natural
pace, with a sentence boundary and technical terms. Measure the resulting WAV;
do not crop or accelerate speech to force exactly 10 seconds. Use one TTS worker
at a time with video rendering idle. Leave NPU turbo off. Pre-render/cache
narration before GPU-heavy video work and test coexistence separately if needed.

Adapt a pinned, inspected official `text2speech.py` sample to the installed API,
with explicit local model/voice paths and an explicit `NPU` device. The rolling
sample docs are not proof that an arbitrary sample revision matches these pins.
In a fresh run directory, the sample's relevant options are
`--device NPU --speaker_embedding_file_path <model>/voices/af_heart.bin
--language en-us --speed 1.0`, plus local model directory and reviewed text.
Inspect its output filename to prevent replacement of an existing WAV.
Use the returned sample rate for the PCM WAV intermediate, measure it, and retain
raw output before any pause/loudness processing. Do not add cloud TTS fallback.

Supervise the synthesis worker externally. A 120-second wall-clock deadline per
short trial is an initial tunable safety bound, not a latency promise. Separate
cold compile/load time from warm generation and audio duration. If the desktop
stutters, playback breaks, the worker hangs, or the driver fails, stop and wait
for that worker to exit before a fallback. Do not overlap an uncertain NPU job
with a retry. Record observations during synthesis and playback, actual listening
intervals, resource/driver errors, elapsed time and any missing evidence. A WAV
or an ASR pass alone does not prove responsiveness or voice quality.

Prefer NPU for this exact configuration only after the device/model/driver checks,
successful explicit-device inference, and the responsiveness/listening trial pass.
Record requested device separately from the real execution evidence. Obtain
compiled-model `EXECUTION_DEVICES` or equivalent backend evidence where exposed;
otherwise keep actual execution unverified and do not mark the NPU gate passed.
Record CPU preprocessing/G2P separately. Avoid `AUTO`, `HETERO` and silent backend
fallback when establishing NPU success. A detected NPU is not execution evidence.

If NPU is unavailable, unsupported, unstable or not verified, attempt the same
model/voice on explicit `CPU` once in a fresh worker with the same deadline.
Use one stream and start with two inference threads, tuning up to four only after
responsiveness checks. The
[CPU scheduling properties](https://docs.openvino.ai/2026/openvino-workflow/running-inference/inference-devices-and-modes/cpu-device/performance-hint-and-thread-scheduling.html)
include `NUM_STREAMS` and `INFERENCE_NUM_THREADS`; configure these through the
installed pipeline's supported constructor properties, not invented sample CLI
flags. Confirm that the adapter accepts/applies the limits; these do not bound all
OS, phonemization or other library activity. Log the NPU failure or skip reason,
CPU request and execution evidence, effective limits, timing and fallback outcome.
Never describe a CPU result as NPU success. If this CPU attempt fails or remains
disruptive, stop synthesis and hand off the reviewed script with the evidence gap.
Do not silently switch to MOSS/Qwen on the laptop GPU.

## Control pace and pauses before audio approval

Use `SpeechGenerationConfig.speed` or the inspected sample's `--speed` for an
audition, beginning at 1.0 and changing only for the requested delivery. Consult
the [generation configuration](https://docs.openvino.ai/2026/api/genai_api/_autosummary/openvino_genai.SpeechGenerationConfig.html).
No documented SSML or native pause control is assumed. Prefer a connected take;
for exact pauses, synthesize reviewed sentence/paragraph clips and insert measured
silence between intact PCM spans. Record boundary offsets and chosen gaps,
review both joins and voice continuity, and retain the originals. The
[existing pause guidance](local-narration.md#continuous-takes-and-sentence-pauses)
gives configurable starting points. Do not interrupt a phoneme or stack pauses.
After audio approval, preserve its pace and retime visuals to the measured WAV.
Realign cues after any authorized audio edit and apply the dedicated voice gate.

Use the [production record](../assets/production-record.md) for exact host,
runtime/assets/licenses, discovery and execution evidence, skipped/failed attempts,
fallback, WAV/cues and listening limits. Laptop tests remain pending until run on
the actual laptop; unit tests of the discovery helper do not validate Kokoro.
