# Teaching opening: TEACH-OPEN-01

For every new video, start by explaining the concept in plain language and
stating concrete learning objectives before introducing the real-world scenario.
The teaching/beginner-comprehension reviewer must explicitly accept or reject
this criterion. It may share the narrative reviewer with a distinct verdict.

## Acceptance rubric

- **Concept baseline:** the first explanatory words say what the concept is in
  ordinary language, without requiring the viewer to know the term already.
  Naming the topic, asking a suspense question or using a jargon-only definition
  is insufficient. Explain unfamiliar terms as they become needed.
- **Concrete objectives:** briefly say what the viewer will be able to explain,
  identify or do by the end, tied to this lesson's actual scope. For example,
  identify the trigger, follow the message and explain a relevant safeguard.
  "Learn about webhooks" is too vague. The stated objectives must be taught.
- **Order:** both belong in the opening before the first real-world scenario.
  A definition or objectives added after the scenario starts fail the opening
  check. When no scenario is used, establish both before the body explanation;
  do not invent a scenario merely to satisfy the rubric.
- **Digestibility:** use a short natural introduction, not a long agenda slide
  or a list of everything the technology can do. No fixed word count, slide,
  duration or number of objectives is required. Allow enough time to understand.

The hook serves this baseline. Let an interesting, accurate plain-language
definition or promise carry the hook; do not put a scenario, suspense-only cold
open or unexplained technical claim ahead of it. A hook alone cannot satisfy
either required element. This user requirement supersedes older hook-first or
human-scenario-first advice for new videos. A named person or concrete situation
can follow the baseline. Preserve useful early interest and honest payoff.

For a narrated video, speak the baseline and objectives; a planning-document
heading, cover or unspoken agenda does not establish their presence. For an
explicitly silent video, use audience-visible text instead; do not add narration.

## Required reviewer evidence

At script/storyboard review, return `criterion_id: TEACH-OPEN-01`, current artifact
identity/hash, `APPROVE`, `REVISE` or `BLOCK`, and separate evidence for:

1. Concept explanation: script path and line range with the actual wording.
2. Concrete objectives: script path and line range with the actual wording.
3. First scenario onset, or body onset when no scenario is used: script path and
   line range, showing the order. State when a scenario is absent.

Reject the opening if either required element is missing, vague/jargon-only or
late. Cite the inspected opening lines that show the gap, and any later occurrence;
do not invent a line for absent content. Describe the smallest needed repair.
Generic "clear introduction" or "good hook" approval is insufficient.

At sample/final review, recheck the actual audience-visible opening and cite
timestamps for the explanation, objectives and scenario/body onset. For narration,
check the actual encoded speech; subtitles or source WAVs alone cannot establish
what the delivered audio contains. A script pass cannot approve an unseen encode.
Record actual listening/viewing coverage and evidence limits; unavailable required
encoded evidence leaves that gate pending, without fabricating timestamps.

Keep this criterion in the existing shared maximum of three total review rounds
per deliverable, including consumed attempts. No new counter, reset, automatic
media regeneration or optional user checkpoint is added. Retain prior evidence
and unresolved findings; material opening edits invalidate affected approvals.
