# Transactional outbox final-v1 QA (NOT PUBLISHED)

Master: transactional-outbox-final-v1-master.mp4 (1080x1920, 24 fps, h264 + aac 48 kHz stereo, 90.375 s)
Cover: publish/transactional-outbox-tiktok-cover-1080x1920.png (1080x1920 RGB, ESRGAN x4 upscale of generated chalk cover)
Caption: publish/tiktok-caption.txt (977 chars, no dashes, lowercase hashtags)

## Audio
- Integrated -14.0 LUFS, true peak -2.8 dBTP, LRA 2.8 LU (ffmpeg ebur128 on the master)
- No scene cut whooshes; music HP 230 Hz, SFX HP 300 Hz; final C add9 chord under end card swell

## Narration (Kokoro am_fenrir 1.20, one take per sentence)
- 251 words; wpm 176.7 over speech span, 166.6 over file, articulation 214.3
- Pauses measured on the MP4 (VAD): sentence n=19 min 0.352 median 0.416 max 0.544; paragraph n=9 min 0.64 median 0.704 max 0.768
- Stem pauses: sentence 0.38 to 0.45 (median 0.42), paragraph 0.68
- No spoken acronyms (acronym hits in STT: none)

## STT on the master (faster-whisper small.en and base.en, beam 5 and greedy)
- Paragraph level: all 10 paragraphs correct on small.en beam 5 except homophones
- Known residual: "polls" heard as "pulls" on 3 of 4 paragraph decodes (sentence level small.en hears polls)
- "Broker down" heard as "broke her down" on base.en sentence decodes only
- Homophones only (no meaning loss in context): writes/rights, sent/scent, and/an, hear/here, or/are, it sent/its scent on base.en
- Whisper greedy hallucination on the short N04S2 clip on one decode only (not in audio)

## Video
- Every frame passed bounds, caption band, label whitelist, brand, limb and causality checks during render; selftest ok
- Encoded frames reviewed: contact sheet every 1 s, 49 phone-size (360x640) frames, every scene transition, relay retry strip, close and end card strip (evidence/enc)
