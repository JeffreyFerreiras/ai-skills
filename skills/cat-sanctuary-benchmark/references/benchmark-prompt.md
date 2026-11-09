# Cat Sanctuary 3D Scene: Benchmark Prompt

Saved 2026-10-25, migrated from the original prompt file. The fixed numbers
replace adjectives so two models produce comparable scenes. Scoring marks
each checklist item pass or fail, then gives one visual score from 1 to 5.

## Prompt

Build a self-contained project folder that renders an interactive 3D cat
sanctuary viewed from above. Download and include every required dependency
so the finished project works without an internet connection.

### Deliverable

- Create a new folder in the benchmark folder (the folder containing
  the prompt file) named
  `<model>-<effort>-<YYYY-MM-DD>`, using your own model name, the reasoning
  effort you were run at, and the local creation date.
  Lowercase the name, turn spaces and underscores into hyphens, and keep
  dots in version numbers. Example: `kimi-k3.1-pro-max-2026-10-25`
- If you do not know your effort level, write `default` instead of guessing.
- If that folder already exists, add `-2`, then `-3`, instead of overwriting
  or reusing another run's folder.
- Put `<model>.html` (your own model name, lowercased with spaces and
  underscores turned into hyphens, e.g. `kimi-k3.1-pro-max.html`) and ALL
  supporting files inside that folder. Keep
  scripts, styles, libraries, and generated assets there, using relative
  paths. Do not depend on files elsewhere on the computer.
- Download a pinned version of Three.js, all imported addons, and every
  transitive dependency into an assets/vendor subfolder. Check that each
  download succeeded and contains JavaScript, not an HTML error page.
- Runtime CDN links are forbidden. If you use an import map, map every
  dependency to a file inside the project folder. Downloads happen during
  creation; opening or running the result must not require the internet.
- No downloaded models, textures, or images. All scene content must be
  procedural. Save any generated assets inside the same project folder.
- Only Three.js. No physics engine, no animation library, no other framework.
- No build step for the user. Include a short `README.txt` with the exact
  launch instructions, the pinned Three.js version, and any runtime needed.
- If ES modules require HTTP, include a launch script for the user's OS
  that serves only this folder on 127.0.0.1 and opens `<model>.html`. State
  the runtime prerequisite in `README.txt` and test the launcher. Do not ask
  the user to disable browser security. If you instead support opening the
  HTML directly from disk, verify that exact launch method in Chrome.

### Camera

- Start at an elevated overhead view: about 60 degrees down from horizontal,
  aimed at the centre of the compound. The layout must read as a plan, but
  walls and cats must still show height and cast shadows.
- Left drag orbits, wheel zooms. Clamp the polar angle between 25 and 85
  degrees so the camera never goes below the ground plane.

### Layout (fixed, so results are comparable)

- A 40 x 30 m walled compound on a grass ground plane.
- Six pens of 10 x 8 m: three along the north edge, three along the south
  edge, with a 6 m central avenue between them.
- Each pen has a 1.2 m fence with one gate gap facing the avenue, a wooden
  shelter, a scratching post, and a water bowl.
- The avenue holds a circular plaza at its centre with one large tree and
  four benches. A small service building with a door and flat roof sits at
  the west end of the avenue.
- A gravel path connects the gates to the plaza.

### Cats

- 12 cats total, two per pen, built only from primitives (body, head, ears,
  tail, four legs).
- Each cat walks a short loop, pauses, and sways its tail. Movement must stay
  inside its own pen at all times.
- Every cat casts a shadow.

### Interaction

- Hovering a pen highlights its fence and shows a label naming it, Pen 1 to
  Pen 6.
- A day/night toggle changes the sun angle, shadow direction, and sky colour.
- A small legend in one corner lists the controls.

### Quality bar

- Holds 60 fps at 1080p in Chrome.
- Under 250 draw calls at the default view.
- One directional sun with shadows plus soft ambient. No unlit materials.
- A seeded random function, so the layout is identical on every reload.

## Why it is built this way

1. Fixed numbers replace adjectives. Six pens, 12 cats, 40 x 30 m, 60 fps,
   250 draw calls. Two models produce comparable scenes instead of two
   different moods.

2. The camera angle is a test, not decoration. 60 degrees with a clamp forces
   correct projection, shadow direction, and object height. Straight down
   lets a model fake the scene with flat quads and still look acceptable.

3. Determinism and a perf ceiling catch lazy solutions. Seeded layout stops
   random churn between runs, and the draw-call cap separates real instancing
   or merging from 400 separate meshes.

4. The project folder carries provenance. Model, effort, and date live in
   its name, and each run keeps its own HTML and assets together. Downloaded,
   pinned dependencies remove runtime CDN availability as a failure point.

Fork in the design: this prompt measures execution. If you want to measure
taste, cut the layout section to "a cat sanctuary" and score the result on a
rubric instead. Keeping both halves lets a model pass every structural check
and still look ugly, or look beautiful and miss half the pens.

## Scoring checklist

Mark each item pass or fail, then give one visual score from 1 to 5.

- [ ] 1. Runs from its benchmark folder using the documented launch method,
       with no console errors and no internet connection. All dependencies
       are downloaded and local; no runtime requests go to external hosts.
- [ ] 2. Six pens present, gates face the central avenue.
- [ ] 3. 12 cats, all inside their pens, all animating.
- [ ] 4. Shadows present and consistent with the sun direction.
- [ ] 5. Hover highlights the correct pen and the label matches.
- [ ] 6. Day/night toggle changes light direction and sky colour.
- [ ] 7. Orbit clamps without clipping below the ground.
- [ ] 8. Holds 60 fps and stays under 250 draw calls.

Visual quality (1 to 5): ___

Notes on where it failed, not just how many checks it missed:

## Critical execution notes

- The project folder, not a single HTML file, is the self-contained unit.
  All required assets must be downloaded or generated into that folder.
- Use the library format and APIs that actually exist in the pinned
  Three.js version. Do not assume a `three.min.js` global build exists, or
  load an ES module as a classic script. Check the actual downloaded files.
- For `Matrix4` transforms, use supported operations such as
  `matrix.multiply(new THREE.Matrix4().makeRotationX(angle))` and
  `matrix.multiply(new THREE.Matrix4().makeTranslation(x, y, z))`. Do not
  confuse Object3D or BufferGeometry transform methods with Matrix4 methods.
- Show a visible error if startup fails. Register error handling before
  dependency loading begins so a failed load cannot leave a blank screen
  or an endless loading message.
- Verify the delivered folder using the documented launch method in Chrome,
  with internet access unavailable while keeping loopback access available
  if a local server is used. Check the console, dependency requests, scene,
  and interactions. Test the actual Three.js library, not only API mocks.
- Report any verification that could not be completed. Do not claim visual
  correctness, successful offline loading, or 60 fps without measuring it.
