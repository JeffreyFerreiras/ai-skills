# Tell one story across the episode

Keep the [teaching opening](teaching-opening.md). Explain the concept and what
the viewer will learn first. Then give the viewer a concrete reason to follow
the mechanism, and answer the same promise in the last teaching beat.

Before outlining the components, write a few sentences in the existing
production document about the starting situation, what matters or is uncertain,
what changes or is decided, and what the outcome means within its limits. The
thread can follow a request, record, system state, or design choice. An ordinary
read, a comparison under a shared constraint, or a choice between two supported
designs can carry the story.

In the storyboard, show what the viewer carries into each important beat and
what they leave knowing. Introduce components when they explain the next change.
Make that change or its result visible, and use it to motivate what comes next.
Across a cut or change of view, keep the relevant item, state, or decision
recognizable. Pay off the opening by showing what someone using or operating
the system sees change, or what a design achieves under the original constraint.
Connect any cost or limit to that outcome. Keep the chain short enough for the
approved runtime and teaching pace.

For a design lesson, the viewer might follow where one service can run when it
must keep serving through the loss of one zone. Show what colocating the copies
leaves exposed, what spreading them changes, and which dependencies still need
protection. The same constraint connects the views and motivates the decision.
The lesson can compare designs without depicting an outage.

## Cache-aside worked comparison

**Component tour.** “The app checks the cache. The cache stores copies. The
database holds product prices. The app writes to the cache and returns the
record. A cached copy can go stale.” Each sentence can match the diagram. The
viewer still has to infer which price a shopper sees and why.

**Connected lesson.** “With cache-aside, the app keeps a copy of data it reads so
it can reuse it. Follow two reads of the same product to see when the app can
skip a database lookup and which price the shopper sees.” A shopper opens
`product:42`. The cache is empty for that key, so the app fetches the $18 price,
saves a copy, and shows $18 on the page. Then the shop corrects the price to $16
in the database only. Another shopper opens the same product while the old copy
is still in the cache. Keep the same key visible across the two requests. Show
the app reuse the old copy on the second request and the page display $18.
“The shop corrected the price to sixteen dollars. The page still shows eighteen
because this read reused the old copy. Reusing the copy saved a lookup, and the
shop needs a way to keep it fresh enough for its prices.” The product, prices
and timing are illustrative.
Ground the mechanism and caveat in the
[cache-aside example](../assets/cache-miss-example.json).

## STORY-01 episode narrative review

In the existing narrative verdict at script/storyboard, rough full timeline,
and encoded final, use the material the viewer receives to reconstruct what
matters at the start, what changes or is decided and why, and how the ending
answers the opening promise. Cite script lines and storyboard scene IDs at the
plan stage, and playback timestamps and actual coverage at the media stages.
Use the intended audience's language and record the reconstruction before
checking it against the production outline. For silent videos use the visible
sequence and text.

Request a focused repair when the reconstruction is a list of parts or adjacent
events, when the next step needs knowledge the viewer was never given, or when
the ending restates the topic without showing or explaining the promised result.
Point to the missing link or payoff and its audience impact. A successful hook,
individual scene, or off-diagram bridge cannot establish the full sequence.
Record any uninspected coverage in the existing gate. This criterion uses the
current narrative reviewer and shared stage budget.
