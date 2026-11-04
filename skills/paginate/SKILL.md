---
name: paginate
description: Explain topics in short pages, one reply at a time, when the user asks for pagination or paced chunks. Use for chat explanations, not software pagination or document layout.
---

# Paginate

Deliver one coherent chunk per reply. Apply this pacing to the requested explanation
and honor later user changes to the format.

## Pages

- Plan coherent chunks before sending the first page so the page count reflects
  the intended coverage.
- Use no more than 90 words per page by default, including headings and the footer.
  Follow a different limit when the user requests one.
- Use plain language. Preserve the details needed to understand the topic,
  including necessary terms, conditions, caveats, and examples. Add pages instead
  of cutting needed meaning.
- Show one page per reply. For a multipage explanation, put an italic page label
  such as `*Page 1 of 3*` near the bottom.
- If pages remain, put `**Next**` alone on the last line, then wait for the user to
  say Next. Treat capitalization as equivalent.
- On the final page, keep the page label and omit Next. A single-page answer needs
  neither a page label nor Next.

## Turn handling

- On Next, show the next page and increment the page number.
- If the user asks about the current page, answer briefly within the same word
  limit without advancing. Repeat the current page label and keep `**Next**`
  available on the last line if pages remain.
- Retain the pending pages during questions. If the user changes the scope,
  adjust the remaining pages and total. Honor explicit requests to change or
  stop pagination.

## Style companions

Follow active style instructions, including any requirement to apply Unslop.
Pagination itself adds no required style workflow.

`unslop` and writing reduction are optional companions unless the user requires
them. Writing reduction means cutting unnecessary words while preserving needed
meaning and the pagination format.

For a separate Reducer and Editor, use `writing-loop` when that workflow is
requested. Its Reducer is a role within the workflow, not a standalone
`writing-reducer` skill. Before using either companion skill, find and read its
`SKILL.md` through the host's skill catalog.
