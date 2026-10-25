---
name: tiktok-copy-cover
description: Write TikTok caption copy and a matching 1080x1920 cover for the current video or topic. Use when caption, hashtags, or cover art is requested, not for video production or publishing.
---

# TikTok copy cover

Produce posting copy plus a real `cover.png` for the current video or topic.
Reuse facts already in chat or the episode brief. Ask only when material
information is absent. Do not regenerate video for a copy/cover-only task.

Read [references/copy-guide.md](references/copy-guide.md) for the copy
deliverables and constraints. Read
[references/cover-style.md](references/cover-style.md) when a cover is needed.
`VIDEO_SKILL_DIR` anywhere in this skill means this skill root
(`skills/tiktok-copy-cover`); never rely on a sibling installed skill.

Default output: one strong hook/title, one ready caption, 3-5 relevant
hashtags (editorial preference, not platform law), one optional pinned
comment when useful, one real `cover.png` (editable SVG kept when the
composition is code-native), and clear file paths. Deliver an actual image
file; never claim a prompt alone is the cover. No publishing, login, upload,
or social messaging unless explicitly requested.

Ground claims in the inspected script or brief. Missing media never counts
as an inspected video. Do not invent timing, coverage, engagement, or
writing-style-match guarantees. Keep facts and review notes out of posting
copy. Before any requested external read or write, verify the expected
source account and notebook identity; never default to another signed-in
profile, and never route around a blocked source.

Cover defaults to PNG 1080x1920 following the bundled reference composition,
phone-safe margins, legibility, and accessibility. Cover style is not video
style. Use available graphics or image generation when appropriate; when no
AI image tool is available in the worker, compose an original code-native
SVG/text/layout rendered with Pillow in the established chalk style. Never
Python-edit an accepted raster original; always compose anew. Do not install
tools or pretend to a capability. Covers follow the bundled gate: actual
viewing of both originals, thumbnail-scale check, independent approval, and
the shared maximum of three cover review rounds.
