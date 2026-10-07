# Instruction writing review

Use this reference when auditing skill instructions or the pointers that route to them. It supports manual review; validator success does not establish instruction quality or model behavior.

## Review the decisions

- **Reference routing:** A pointer names the material and the distinct condition that calls for it. Check that each supported branch can reach its needed reference without loading unrelated branches. For example, “For tracked changes, read redlining.md” gives a branch trigger; “See more guidance” does not.
- **Completion:** Give consequential steps an observable end condition. “Compare each selected file with the base revision and account for its behavior changes” is checkable; “understand the diff” leaves completion unclear. Keep criteria within the user's task, rather than requiring exhaustive work beyond it.
- **Related rules:** Keep a term's definition, applicable rules, exceptions, and caveats together. Retain scoped conditions and authority boundaries when shortening prose.
- **Maintained facts:** Prefer the actual configuration, command help, or repository layout for cheap lookups. Document the reason or gotcha those sources do not explain. Retain an explicit runtime or permission constraint when it changes a decision, even if configuration also records it.
- **Disclosure:** Keep shared purpose and essential constraints in the entrypoint. Move substantial detail needed only for a particular branch to a reference, with an explicit trigger at the call site. A short, self-contained workflow needs no extra reference layer.

## Record the result

For each material issue, name the affected passage, a realistic request that exposes it, and the decision that needs clarification. Distinguish a wording concern from an observed execution failure. When no live scenario ran, report behavior as unverified; these techniques alone do not prove better prompting. Use existing task checks and authorization, rather than adding a new mandatory review stage.

## Source

These are concise local adaptations of [mattpocock/skills writing-for-agents](https://github.com/mattpocock/skills/blob/6fd947921b935b7e1e69293a200400f0fdd5c15f/skills/productivity/writing-for-agents/SKILL.md), pinned at `6fd947921b935b7e1e69293a200400f0fdd5c15f`. They borrow branch-aware pointers, completion criteria, grouped concepts, environment-backed facts, and progressive disclosure. Upstream's broader claims about model behavior are not adopted as verified results.
