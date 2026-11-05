# 3D and pixel-art styles

Use these optional treatments when selected or when the user asks for creative
style choices. Use the [visual style picker](style-gallery.md) to show saved
samples and obtain the user's choice. Keep [Neon Chalk Systems](neon-chalk-systems.md)
and [Dark drafting desk](dark-drafting-desk.md) available. Preserve an episode's
chosen style unless the brief changes it.

## Choose for the lesson

Offer a few relevant choices with a teaching reason for each. Sculpted studio
suits approachable introductions. Glass cutaway exposes hidden mechanisms.
Pixel circuit makes discrete states easy to distinguish. Voxel workshop combines
block-built depth with a playful miniature setting. These are starting points,
not a requirement to use one scene template or palette for every topic.

Use one primary treatment per episode. A contextual bridge can use a different
treatment when the shift helps the explanation. Preserve object identities,
semantic colors and the return to the mechanism. Record the selected treatment,
reason, actual rendering method and inspected sample in the production document.
For an open brief, recommend a treatment and wait for the user's choice through
the picker. Do not infer a choice from silence or from a different episode.

## Sculpted studio

Inspect the selected variant's saved sample:
[light](../assets/style-references/sculpted-studio.png) or
[dark](../assets/style-references/sculpted-studio-dark.png).

Use tactile matte ceramic or resin, rounded architectural forms, grounded contact
shadows and soft studio lighting. Start with warm ivory `#F2E5D5`, ink `#263330`,
seafoam `#91B39B` and terracotta `#C65B32`. These are proposed working tokens,
not measured swatches from the sample or universal state meanings.

For dark mode, use a charcoal-blue background and slate ceramic housings. Keep
the seafoam cards and terracotta accents. Soft warm key lighting and restrained
cool edge lighting should separate the forms without neon glow or crushed blacks.
The dark sample changes palette and lighting while retaining the light sample's
composition and rounded forms. Record which variant the user selected.

Give each role a distinct silhouette. Show a queue as a visible rack, a processor
as an open work cradle, or a storage boundary as a compartment when that metaphor
fits. Keep labels upright in screen space and leave the active object unobscured.
Use a fixed orthographic or shallow-perspective teaching view. Soft materials
must still have enough edge contrast to read on a phone.

For motion, move one identified job into the rack, let the worker take it, then
show the resulting state after processing. This is a possible sequence, not a
claim that all queues are ordered or durable. Avoid bouncy toy motion that
obscures the moment of receipt or completion. Use this treatment for onboarding,
service boundaries and tangible explanations of otherwise abstract operations.

## Glass cutaway

Inspect the [Glass cutaway sample](../assets/style-references/glass-cutaway.png).

Use smoked-glass shells, graphite bases, a few exposed internal parts and
controlled reflections. Start with midnight `#071322`, graphite `#283440`,
cyan `#57CFE1` and amber `#F6B94B`. Keep the active record opaque and bright enough
to distinguish from the shell. Transparency should reveal the relevant mechanism;
remove a wall or use an opaque inset when overlapping surfaces obscure it.

Use this treatment for internal buffers, cache lookup, storage layers or an
explicitly illustrated processing stage. Separate layers only when the model
contains those distinctions. A glowing part has no success, persistence or
security meaning until the lesson establishes it.

Animate the operation inside the enclosure and hold on its result. A cutaway
can reveal where a record waits or which part changes after an operation.
Keep connector endpoints and direction visible through the shell. Avoid repeated
reflections that look like additional records, bloom over labels, or an orbit
that hides the action. Dark glass needs inspection after compression.

## Pixel circuit

Inspect the [Pixel circuit sample](../assets/style-references/pixel-circuit.png).

Use deliberate square-pixel clusters, stepped contours, a small palette and
distinct sprites for the states the lesson needs. Start with ink `#19152D`,
cream `#F4E3B1`, mint `#91D5BE`, gold `#F2BB52` and coral `#C97670`.
Keep texture sparse and behind the mechanism. Avoid scanlines, CRT distortion,
tiny bitmap text and flashing effects that reduce readability.

For a 1080x1920 output, a 270x480 art layer scaled 4x or a 180x320 layer scaled 6x
gives a consistent grid. Use nearest-neighbor sampling and integer art-layer
positions. Disable smoothing for that layer and avoid fractional camera scaling
or rotations that make its pixels shimmer. Keep subtitles and detailed labels
at native output resolution with a readable font. The final video can remain
30 fps; a pixel look does not require choppy timing or faster speech.

Use this treatment for queues, retries, finite-state machines and stepwise
algorithms. Change a sprite's state after its causal event. Mark the change with
color plus a shape change or label. Preserve the same job identity through a
retry. A rack of sprites does not by itself establish FIFO, capacity, or delivery
guarantees. Generated pixel-style artwork is a visual reference, not proof of a
consistent pixel grid or a usable sprite sheet.

## Voxel workshop

Inspect the [Voxel workshop sample](../assets/style-references/voxel-workshop.png).

Use solid cubic forms, stepped silhouettes, broad faces and grounded shadows.
Start with dark teal `#173E42`, muted teal `#5D8F88`, cream `#EBD6B5`,
honey `#F4BB43` and orange `#CA622D`. Give jobs, workers and boundaries different
silhouettes. Keep the world small enough that the active objects remain large.

Use this treatment for partition placement, work distribution, bounded buffers
or a contextual miniature that leads back to a precise diagram. Explain what a
block represents. Stacking blocks must not imply capacity, redundancy or a real
machine count unless the source or an explicit illustrative assumption supports it.

Let an identified block move, occupy a slot or change state in response to an
event. Reveal an added worker only when the explanation reaches it. Keep the
camera stable during the causal step; use a measured view change only when it
reveals a boundary. Avoid decorative cityscapes, extra conveyor branches and
occlusion by tall blocks. Voxel geometry and 2D pixel art are separate methods;
choose crisp stepped silhouettes without forcing the pixel-art scaling rules
onto a true 3D scene.

## Implement the selected treatment

Separate visual appearance from the rendering method. A generated picture with
3D shading is raster artwork. Layered artwork with parallax is 2.5D. Call a scene
true 3D only when it uses editable geometry, a camera and lighting. Reuse the
project's renderer; use available Three.js or Blender geometry when actual
depth or changing views justify it. Selecting a style does not install a new
renderer or authorize a paid service. If geometry is unavailable, disclose the
raster or 2.5D approach before promising rotation or moving internal parts.

Keep labels, captions and technical connectors editable in the compositor.
Generate separate subjects and backgrounds when independent movement is needed.
Drive geometry, sprite states, lights and transforms from the event timeline;
seed any noise and keep the same frame reproducible. Material changes and camera
movement cannot substitute for an observable causal state change.

Match semantic colors to the episode, pair them with another cue, and check
contrast at the final size. Inspect the entry, action, result and cutback for
occlusion, connector continuity, stable identities and caption collisions.
Check reflections and shadows in dimensional styles, and pixel stability in
pixel art, during motion and in the actual encoded file.

## Sample scope and provenance

The five linked PNGs are AI-generated still explorations created with the built-in
imagegen tool on November 4, 2026. The [sample manifest](../assets/style-references/style-options-manifest.json)
records the exact prompts, dimensions and hashes. They illustrate a generic
photo-processing queue to make the treatments comparable. Their card counts,
machine shapes and pathways are illustrative, not evidence about a real system.
The paths have no arrowheads; direction and queue semantics need authored labels,
connectors and actual causal events in an episode.
Several machine entries also hide path connections and height changes. Rebuild
those connections and distinguish job identities before animating them.

These images show material, palette and composition ideas. They have no narration,
captions, temporal evidence, editable geometry or user style acceptance. Their
portrait dimensions are recorded as generated, not certified 1080x1920 frames.
Inspect and recompose for the delivery aspect ratio instead of stretching them.
Do not reuse a complete image as a finished mechanism or animate it only by
panning and zooming. Create the episode-specific assets and state changes.

A request only for style choices can end with clearly labeled still samples.
When a sample teaches episode content or demonstrates motion, use the existing script,
storyboard, scene, sample and encoded-review gates for that scope. The user-requested
style choice adds no independent-review round and grants no exception to the
narration, story-bridge or cover requirements. A still does not pass a
representative video sample gate.
