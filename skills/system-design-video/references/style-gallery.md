# Visual style picker

For a new episode without a chosen style, show the saved samples below and ask
one question: "Which style should this video use?" Keep the labels and names
beside their images so the user can answer by letter or name. You may recommend
one with a short teaching reason, but wait for the user's answer before visual
production. A default option, elapsed time or silence is not a choice.
Continue source research and style-independent script planning while waiting.

An explicit style in the request, a recorded choice for this episode, or an
explicit request to choose on the user's behalf satisfies the choice. Do not ask
again on continuation. General preferences and earlier episodes do not select
the style for a new episode. If the user delegates the decision, state the
selected treatment and show its saved sample.

## Show the images when asking

Resolve these files relative to the installed skill, inspect the actual pixels,
and display the previews in the user-visible conversation immediately before
the question. Use image attachments or inline image output that the host
supports. Agent-only image inspection, style names, filenames and download links
alone do not satisfy the visual choice. If a question tool accepts only text,
show the images first, then use the same labels in its question or choices.

The picker shows one lead still per option. Supplemental frames for an option can
be shown on request, before the user answers.

Reuse the saved images rather than regenerating them for each invocation. Keep
their aspect ratios and make each sample large enough to compare materials and
composition. If the host cannot display them, explain that limitation and keep
the choice pending until the user can view the samples or explicitly delegates
the decision. Do not silently choose the historical default.

Record the displayed sample paths/hashes, the user's answer and the selected
variant in the production document. Keep style selection separate from video
acceptance. These stills do not establish animation, narration or causal accuracy.

## A. Sculpted studio, light

Warm ivory ceramic, soft shadows, seafoam and terracotta accents.

![A. Sculpted studio in light mode](../assets/style-references/sculpted-studio.png)

Supplemental samples: [Request routing](../assets/style-references/extra/sculpted-studio/request-routing.png),
[Edge cache](../assets/style-references/extra/sculpted-studio/edge-cache.png).

## B. Glass cutaway

Dark transparent housings with visible internal parts and amber records.

![B. Glass cutaway](../assets/style-references/glass-cutaway.png)

Supplemental samples: [Read replicas](../assets/style-references/extra/glass-cutaway/read-replicas.png),
[Storage tiers](../assets/style-references/extra/glass-cutaway/storage-tiers.png).

## C. Pixel circuit

Retro sprites and stepped contours on a deep purple field.

![C. Pixel circuit](../assets/style-references/pixel-circuit.png)

Supplemental samples: [Forest market receipts](../assets/style-references/extra/pixel-circuit/forest-market.png),
[Neon street courier](../assets/style-references/extra/pixel-circuit/neon-street.png),
[Workbench drive tray](../assets/style-references/extra/pixel-circuit/drive-tray.png).

## D. Voxel workshop

Block-built forms, grounded shadows and warm illuminated machinery.

![D. Voxel workshop](../assets/style-references/voxel-workshop.png)

Supplemental samples: [Blockwork router](../assets/style-references/extra/voxel-workshop/blockwork-router.png),
[Cache eviction](../assets/style-references/extra/voxel-workshop/cache-eviction.png).

## E. Dark drafting desk

Dark slate, precise ivory lines, copper and sage accents.

![E. Dark drafting desk](../assets/style-references/dark-drafting-desk.png)

Supplemental samples: [Cache hit](../assets/style-references/extra/dark-drafting-desk/cache-hit.png),
[Dead-letter queue](../assets/style-references/extra/dark-drafting-desk/dead-letter-queue.png).

## Neon Chalk Systems

Dark charcoal field, chalk-like contours and restrained luminous accents.

![Neon Chalk Systems, card tap](../assets/style-references/extra/neon-chalk-systems/card-tap.jpg)

Additional payments frames, shown on request: [Approval path](../assets/style-references/extra/neon-chalk-systems/approval.jpg),
[Consensus reached](../assets/style-references/extra/neon-chalk-systems/consensus.jpg),
[Capture](../assets/style-references/extra/neon-chalk-systems/capture.jpg),
[Clearing](../assets/style-references/extra/neon-chalk-systems/clearing.jpg),
[Payment receipt](../assets/style-references/extra/neon-chalk-systems/payment-receipt.jpg),
[Inventory order form](../assets/style-references/extra/neon-chalk-systems/order-form.jpg),
[Interbank net settlement](../assets/style-references/extra/neon-chalk-systems/net-settlement.jpg),
[Institutional ledgers and seller deposits](../assets/style-references/extra/neon-chalk-systems/ledger-and-deposits.jpg).

See [Neon Chalk Systems](neon-chalk-systems.md) for the full style guide.
Provenance: the still is a frame from the payments sample video, not a generated
style exploration.

## Other styles and references

[Neon Chalk Systems](neon-chalk-systems.md) is available as an additional visual
choice with the still above. Before offering any other style, obtain and show an
actual reference for that treatment. A supplied user reference can also define
the episode's style.

Use [3D and pixel-art styles](style-options.md) or
[Dark drafting desk](dark-drafting-desk.md) for the selected treatment's directions.
The [sample manifest](../assets/style-references/style-options-manifest.json)
preserves the initial four generated samples' prompts, dimensions and hashes. It
does not cover the supplemental frames added later, which are copied unchanged
from their reviewed sources. The drafting-desk reference has its own provenance
in its style guide. Keep the source images in the skill package so future
invocations can show the same references.
