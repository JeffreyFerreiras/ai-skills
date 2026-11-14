# Transactional outbox, worked example

Approved script, canonical narration, compiling-card excerpts, QA record, TikTok caption, and cover for the chalk TikTok on the transactional outbox. This is one episode. It is not a template you copy over a new topic, and it is not a re-render of the master. The master MP4 is not in the skill.

Read [the series workflow](../../chalk-tiktok-series.md) before using these files on a new episode.

## What is here

- [script-v1.md](script-v1.md): sourced script and storyboard. Timestamps in it were estimates. The QA report has the measured master.
- [canonical-narration.txt](canonical-narration.txt): the spoken words. "Entity Framework Core" first, then "Entity Framework". No spoken acronyms. No em dashes.
- [code/](code/bad.cs): on-screen excerpts. `bad.cs`, `good-outbox.cs`, and `relay.cs`. Each line is at most 34 characters. They were pasted into a net9.0 EF Core Sqlite program that compiled with warnings as errors and exercised the lost event, the joint commit, the relay retry, and the duplicate. That program is not bundled. Do not describe the fragments as a complete compiling file by themselves.
- [QA-REPORT.md](QA-REPORT.md): historical QA for the 90.375 s, 24 fps, 1080x1920 master. Loudness -14.0 LUFS, true peak -2.8 dBTP. Fenrir at speed 1.20. The report left "polls" heard as "pulls" on some paragraph decodes. The series rule now regenerates a meaning-changing mishear like that. The report stays as the record of what that pass found.
- [tiktok-caption.txt](tiktok-caption.txt): the paste-ready caption. Deliver a caption as a message that contains only this kind of text.
- [transactional-outbox-tiktok-cover-1080x1920-v2.png](transactional-outbox-tiktok-cover-1080x1920-v2.png): the approved cover. Headline, diagram, and gigachad, restacked into the 3:4 band.
- [compose_cover_v2.py](compose_cover_v2.py): how v2 was restacked from a v1 cover. The v1 pixels are not bundled, so the script cannot be re-run from this folder alone.

## Pipeline

Render, sound, pauses, speech-to-text, encode QA, and loudness live in the [reference pipeline](../../../scripts/reference/transactional-outbox/README.md). Narration WAVs, word timings, the laptop plate, and the sound helper are episode files and are not in this package.

Sources for the claims are the Microsoft Learn pages listed in the script. Use those pages, not this example, as the evidence for a new episode.
