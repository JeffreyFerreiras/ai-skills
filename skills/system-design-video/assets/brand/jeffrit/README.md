# Jeffrit brand end card

Hand-drawn chalk "coding gigachad" bust of Jeffrey, in the same chalk style as the episode chalk people, plus the word "Jeffrit". No photo cutout is composited into any output. The profile photo in `src/` is the likeness reference only.

## Files

- `jeffrit-bust.png`: keyed head-and-shoulders drawing (RGBA). Use the whole bust as the developer. Do not paste the face onto another body. The jaw is too wide for other bodies. On a 1080-wide frame, draw it about 340 px wide so it reads on a phone.
- `jeffrit-badge.png`: round chalk badge. The head pops out above the ring (RGBA).
- `jeffrit-wordmark.png`: "Jeffrit" chalk lettering with the underline (RGBA).
- `jeffrit-logo.png`: badge over the wordmark (RGBA), for a still lockup.
- `jeffrit-endcard-preview.png`: end card still on a plain dark board.
- `jeffrit-endcard.mp4`: standalone clip, 1080x1920, 24 fps, 3.375 s (81 frames). The picture sits on flat RGB 17, 25, 29. The audio track is silent (measured mean and max about -91 dB). The in-episode animation is the 2.9 s `EndCard` class, crossfaded after the laptop closes. This clip is the reusable reference, not a second timeline.
- `layers/`: animation layers used by `endcard.py`.
- `build_assets.py`: rebuilds the PNGs from `src/jeffrit-gigachad-raw.jpg`. The rebuild looks for Caveat Brush under the usual system font directory. Deterministic seeds.
- `endcard.py`: `EndCard` class. Local timeline, in seconds: crossfade in 0 to 0.6, ring 0.3 to 0.95, bust pop 0.6 to 1.0, lettering 1.02 to 1.95, underline 1.90 to 2.25, jaw glint at 2.2, fade over the last 0.45 s. Length 2.9 s. The only text drawn is "Jeffrit".
- `src/jeffrit-gigachad-raw.jpg`: chalk drawing used to build the keyed bust.
- `src/jeffrey-meme-original.png`: Jeffrey's profile photo, kept because he asked for it in the skill. It is the likeness reference. Do not put it on screen.

## Use

Resolve this directory from the installed skill and import `EndCard` from it. Pass the episode chalkboard, 1080x1920 RGB. Call `frame` with time local to the card and the last frame of the laptop close as `prev`, so the card crossfades in. `cues` returns SFX times for the ring, pop, letters, underline, glint, and fade.

`sha256-manifest.txt` lists every file in this directory except itself. Regenerate it when a shipped byte changes.
