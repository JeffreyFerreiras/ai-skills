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
2. Spawn one new reviewer with fresh context. When `collaboration.spawn_agent` is available, use
   `fork_turns: "none"` and prefer the `code_reviewer` role for code changes. For other artifacts,
   choose an available review-capable subagent. Honor an explicit model or effort constraint; if
   the fixed reviewer role conflicts, use an exact supported selection with read-only instructions
   or report the limitation. Never silently substitute. If no subagent can run, say that an
   independent review did not occur.
3. Give the reviewer the target location and scope, intended behavior or acceptance criteria,
   relevant repository constraints, and available validation results. Do not supply the author's
   reasoning, a preferred verdict, or a prior review conclusion. Instruct the reviewer to inspect
   surrounding context as needed, make no edits, and spawn no further agents.
4. Request actionable findings with severity, location, triggering condition, impact, and a focused
   remedy. Require an explicit statement when no findings remain, plus any unresolved questions or
   validation limits. Wait for completion and report the reviewer's result faithfully, distinguishing
   its findings from your own judgment. Do not claim that tests or live behavior were independently
   verified unless the reviewer actually checked them.

If the user asks for fixes after the review, treat that as a separate implementation request.
