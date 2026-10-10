---
name: produce-explainer-video
description: Produce narrated technical explainer videos from grounded sources, with a representative sample, concept-specific scenes, independent reviews, and reproducible local delivery.
---

# Produce an explainer video

Create a complete teaching video and recoverable sources. Default to free local tools, native Kokoro `af_heart` at speed 1.0, and a vertical phone-readable export. Honor explicit voice, format and style choices. Detect capabilities before promising a render. No model weights, private data or reference artwork belong in this skill.

## Production sequence

1. Inspect the existing topic catalog and artifacts. Use one current tracker and one current production document. Preserve accepted work; distinguish a new topic from a remake. Start from [the production template](assets/production.md) and [tracker schema](assets/tracker.json).
2. Ground claims in actual primary/reference passages. Write the production document before rendering: audience, learning outcome, scope, source anchors, practical implementation, assumptions and limits. Open with **what we learn and why it matters**, then the real-world actors, goal, failure and stakes. Define terms, show normal operation, then failures, tradeoffs and recap. Complete explanation determines duration; no arbitrary cap.
3. Draft the narration and storyboard. Give each explanation a fitting scene or diagram, visible input/current/output states, spoken cue, causal action and transition. One continuing example gives continuity; one diorama with camera zooms does not give enough explanatory variety. Read [production and review gates](references/production.md) for independent technical and editorial responsibilities.
4. Build a **representative narrated sample for every episode, always before full production**. Include actual motion, screen-space karaoke captions, a transition and an explanatory diagram. A still, silent intro or another episode's sample cannot pass this gate. Review it at native pace, obtain meaningful sample feedback, iterate, and record the selected configuration. Reuse existing authorization when it already covers proceeding.
5. Implement the full timeline with [visual and motion guidance](references/style.md). Preserve useful layouts, narration and timing during depth polish. Use polished isometric context scenes, subtle raised diagrams, dark slate/cyan/lavender, level labels, internal actor/screen/LED action, quiet event sounds and fine curved directional light paths. Stable teaching holds matter.
6. Checkpoint exact art, script, WAV, cues, dependencies and code before long rendering. Run a **native-paced animatic editorial gate** plus every-frame caption/crop/path/glow/actor audits before expensive encoding. Review transitions and transit states. Freeze repaired sources; prior-cut reviews do not approve changed code.
7. Render bounded work units from the frozen source lock. Follow [local tools and contracts](references/tools.md) for portable helpers, capability detection, source-set checks, exact WAV preservation, alpha/camera/overlay contracts and stop/resume behavior. Never shorten or stretch narration to fit frame boundaries.
8. Independently review the **encoded master and phone copy**, with separate editorial and technical verdicts. Correct issues and recheck the repaired scope until both pass. Technical correctness alone does not establish an engaging, varied video. If independent reviewers or playback are unavailable, report the missing gate; never relabel self-review as independent.
9. Deliver playable video, production document, captions, exact WAV/cues and verified recovery ZIP. Save to the user's requested destination and preserve existing Library identities when applicable. Update the existing tracker and guide with evidence. A publishing title/description is a draft when requested. **Delivered, user-ready and published are separate states. Publish only with authorization.**

Every report must distinguish frame sampling, automated full decode, continuous watch/listen, waveform checks and independent forced alignment. Kokoro cues are model estimates. Report inspected timecodes, limits and remaining blockers, without claims of watching or listening unsupported by actual playback.
