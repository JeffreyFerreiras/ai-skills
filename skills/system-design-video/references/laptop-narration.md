# Laptop narration

Use the [Kokoro CPU workflow](kokoro-narration.md) for new local narration on the
Core Ultra 9 / RTX 5070 laptop as well as the desktop. CPU is the default;
accelerator detection does not select NPU or GPU. Keep MOSS/Qwen as explicit
alternatives and preserve existing approved takes and episode overrides.

Record the actual laptop CPU SKU, OS, driver and storage before setup. An
environment-specific test result or path from another machine does not establish
this host's configuration, responsiveness or voice quality. "Intel AI Boost"
alone does not establish model compatibility. The NVIDIA GPU and OpenVINO's Intel
`GPU` device are distinct.

Start with a roughly 10-second explicit CPU fixture under the documented thread,
affinity and worker deadline limits. Measure its WAV, check system responsiveness,
inspect pronunciation and audition the actual selected voice. Record startup and
generation separately. Do not regenerate accepted episodes to test a new engine.

NPU work requires a separate explicit request and supported exact model/driver/
device evidence. It is not a default attempt or an automatic fallback from CPU.
Retain any prior device-loss/stutter evidence with its host and configuration;
do not generalize its cause or retry an unstable graph as routine narration.
Official [system requirements](https://docs.openvino.ai/2026/about-openvino/release-notes-openvino/system-requirements.html)
and [NPU documentation](https://docs.openvino.ai/2026/openvino-workflow/running-inference/inference-devices-and-modes/npu-device.html)
describe support prerequisites, not a completed stability test.

The optional [discovery helper](../scripts/narration_probe.py) records available
OpenVINO devices without loading a model. Discovery is not inference, actual
execution-device evidence or approval to use an accelerator. Its pending NPU
validation is intentional. CPU default narration does not require an NPU probe.
