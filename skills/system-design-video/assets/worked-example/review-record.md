# Worked-example review record handoff

Episode `put-retry-unchanged`. Scenes `intro`, `shop-sign`, `retry`.
Plan bridge `shop-sign`, frames 60-120 of 180 at 30 fps.

## Sample choice

Use `shop-sign` sample frames only. No full opening render is needed.
Named frames: `before` 30, `entry` 65, `action` 90, `exit` 115, `after` 130.
Inspect these stills for the sample decision.

## Final opening needs

Full 180-frame render of all three scenes.
Spec captions for `intro`, `shop-sign`, and `retry`.
Bridge plan with `shop-sign` continuity anchor.
Encoded MP4, decoded stills, and `media-report.json`.
Captured `scene-bridge-encoded.json` from the actual encode.

## Visual response coverage

Scope: `shop-sign` bridge window and continuity anchor.
Artifacts: prepared `video-spec.json`, bound `src/scene-artwork.jsx` hash,
captured `scene-bridge-encoded.json`, sample stills.
Check: sign state matches stored value `v1` across repeat.
Verdict: PENDING. Fill after production.

## Logical response coverage

Scope: PUT replacement and repeat side-effect claims in spec.
Artifacts: prepared `video-spec.json`, claim ledger entries,
`shop-sign` story action and semantic link.
Check: retry shows the same stored value with no new effect claimed.
Verdict: PENDING. Fill after production.

## Still pending

Actual playback. Actual timecodes. Actual file hashes.
Independent visual verdict. Independent logical verdict.
Final `media-check` result. No review or render receipt is recorded here.
