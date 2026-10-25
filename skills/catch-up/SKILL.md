---
name: catch-up
description: Give a concise in-chat recap of the active conversation when the user asks to catch up, where they were, what they were doing, or for a conversation recap. Use only the current conversation and supplied context.
---

# Catch Up

Recap the active conversation in chat. Do not resume execution.

## Sources

Use only the current conversation and context or summaries the user supplies. Do not read session storage, other threads, repository files, or tools to reconstruct it.

Respect the latest correction and priority. Drop goals the user abandoned or replaced. Treat a proposal, plan, or attempt as unfinished unless evidence shows the outcome.

## Build the recap

Cover these four points:

- Current goal, in one sentence.
- Confirmed completed work.
- What remains, or what blocks it.
- One useful next action.

Separate user decisions from assistant proposals and unverified attempts. Do not present a recommendation or a prior attempt as completed work.

When context is partial in a way that changes the answer, name the specific gap in one line. Do not claim a complete transcript.

## Style

Default to a short recap that reads at a glance. Follow the host and user style; short paragraphs or a small list both work. Include only the reasons, links, identifiers, and blockers needed to resume. Add detail only when requested.

If the work is finished, say so. Do not invent pending work or a next step.

Do not narrate tools, dump logs or transcripts, or add approval loops. A recap request alone changes nothing: do not run investigations, create files, save vault notes, send messages, or load handoff or vault-capture-chat. If the user also asks for fresh verification or an action, do that explicitly and label it as separate from the recap.
