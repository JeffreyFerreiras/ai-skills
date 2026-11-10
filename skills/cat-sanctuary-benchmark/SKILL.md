---
name: cat-sanctuary-benchmark
description: Run or grade the cat sanctuary 3D scene benchmark for AI models. Use when asked to benchmark a model on a Three.js scene, run the sanctuary experiment, or score a benchmark deliverable.
---

# Cat Sanctuary Benchmark

Measure execution, not taste. A fixed prompt asks for a procedural Three.js
scene with fixed numbers, so two runs produce directly comparable results.
The prompt, its design rationale, and the critical execution notes are the
source of truth: read [references/benchmark-prompt.md](references/benchmark-prompt.md)
before any run or grade.

## Build mode (the agent runs the benchmark)

1. Pick the results folder. Use the prompt file's parent folder when one
   exists. Otherwise create a fresh benchmark workspace, save a copy of the
   prompt file into it, and treat that folder as the results root.
2. Follow every section of the benchmark prompt exactly: fixed layout
   numbers, camera clamps, 12 primitive cats, hover and day/night
   interactions, seeded randomness, pinned local Three.js, no CDN or
   downloaded media, no other libraries.
3. Name the deliverable folder and HTML file per the prompt's naming rules
   so model, effort, and date stay in the paths. Never reuse another run's
   folder; add `-2`, `-3` instead.
4. Verify with Chrome before scoring (see Verification).

## Grade mode (score an existing deliverable)

1. Confirm the folder carries its model, effort, and date in its name, and
   that `README.txt`, the HTML, and all vendor assets live inside it.
2. Launch with the folder's documented method in Chrome. Use available browser
   tools to collect console, network, fps, and draw-call evidence.
3. Mark each checklist item in the benchmark prompt pass or fail with what
   you observed, then give one visual score from 1 to 5.
4. Report where it failed, not just how many checks it missed. Say plainly
   which verifications could not be completed; never claim 60 fps or
   offline loading without measuring it.

## Verification

- Launch exactly as the deliverable's `README.txt` documents. If no method
  is documented, default to a loopback static server and note the gap.
- Check the console for errors and the network log for requests to
  external hosts. Loopback requests are allowed only when a local server
  is used.
- Exercise hover labels, the day/night toggle, and the orbit clamp to both
  ends of its range.
- Measure fps and draw calls at the default view:
  `renderer.info.render.calls`, or the Chrome performance and GPU-tools
  panels when the code does not expose the counter.
- Test the scene as built. API mocks and stubbed interactions do not count.
