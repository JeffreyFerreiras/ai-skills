---
name: independent-reviewer
description: Spawn a fresh, non-author subagent to review completed work or a specified diff or artifact when the user asks for an independent agent review. Do not use for an ordinary solo review or a write-and-repair loop.
---

# Independent Reviewer

Use this skill when the user explicitly asks for a subagent, second agent, or independent reviewer.
The reviewer gives a read-only assessment; this skill does not authorize repairs.

1. Resolve the review target from the user's request. If none is named, use the changes from the
   active task. Check repository instructions and status, identify the exact diff, commit range, or
   artifact, and exclude unrelated changes. Wait until the author has stopped editing before review.
2. Spawn one new reviewer with fresh context. By default, inherit the calling agent's model and
   reasoning effort, independently of the author worker's settings. When `collaboration.spawn_agent`
   is available, use `fork_turns: "none"`; omit `model` and
   `reasoning_effort` so both inherit. Apply any explicit user reviewer model or effort override
   independently, leaving an unspecified setting inherited. Do not use a role with fixed model or
   effort that would override these settings. If the backend cannot honor the requested settings,
   report the limitation before launch; never silently substitute. If no subagent can run, say that
   an independent review did not occur.
3. Give the reviewer the target location and scope, intended behavior or acceptance criteria,
   relevant repository constraints, and available validation results. Do not supply the author's
   reasoning, a preferred verdict, or a prior review conclusion. Instruct the reviewer to inspect
   surrounding context as needed, make no edits, and spawn no further agents.
4. Request actionable findings with severity, location, triggering condition, impact, and a focused
   remedy. Require an explicit statement when no findings remain, plus any unresolved questions or
   validation limits. Wait for completion and report the reviewer's result faithfully, distinguishing
   its findings from your own judgment. Do not claim that tests or live behavior were independently
   verified unless the reviewer actually checked them.
   The review gate passes only when no actionable findings or blocking evidence gaps remain;
   unresolved findings, incomplete review, or an unavailable reviewer never count as approval.

If the user asks for fixes after the review, treat that as a separate implementation request.
