# Choose the view that explains the mechanism

Change views when the viewer's question changes. A chapter should progress through the views its explanation needs, rather than meet a quota of different layouts. Keep the example, key, colors and actor names consistent across them. Introduce rewinds explicitly and restore the starting state when comparing alternative policies.

The starter defaults to clean cuts (`transition_seconds: 0`) so the board stays stable and a new view is immediately readable. Inspect each boundary in encoded frames. If a transition needs a crossfade, author it between complete scenes; fading every new view up from black creates distracting dark dips.

The [editable starter](../assets/scene.py) supports these building blocks:

Nodes and comparison items accept `shape`: `browser`, `copy`, `database`, `tag`, `receipt`, or the default `panel`. These silhouettes identify roles without moving labels. Nodes can set `muted: true` for an unchanged or unused component, while retaining its stored value. Author the transfer through the actual responsible actor; for cache-aside, show database-to-application and application-to-cache, not an automatic database-to-cache link.

Check causal timing before encoding: a request must arrive before its reply departs, a copied value must reach the application before its cache write starts, and a receiver's value must change at or after arrival. Derive these dependencies from one event table and assert their order. A spoken cue can start an operation; it must not make a completed result appear while the corresponding packet is still traveling. Inspect just before, during and after each transfer, not only the completed diagram.

- `diagram`: request or data flow. Use `nodes` with concise `states`, named directional `paths`, and cue-aligned arrival/state changes. Show the returned value and the component left untouched on a cache hit. Traveling dots alone do not teach a result.
- `comparison`: exactly two `comparisons`, each with `id`, `label`, optional `detail`, timed `states`, and optional `value_size` (default 42 design units). Use it for before/after, divergent copies, or a decision with two consequences. The state is large and the label stays fixed. Reveal differences when they occur in narration, not in advance.
- `timeline`: `events` with increasing numeric `time` values, scene-relative reveal `at`, `label`, and `value`. Positions preserve relative time, while alternating text lanes separate nearby events. Use `note` for the time scale or simplifying assumption. Dense events can still collide; recompose them and run the geometry audit.
- `context`: a small actor and terrace for orientation. Replace this starter art with episode-specific actors where the scenario needs them.

Example scene data, with `start` and `end` taken from actual speech:

```json
{"start":0,"end":12,"type":"comparison","title":"A hit can return an old value","comparisons":[
  {"id":"cache","label":"Cached copy","detail":"product:42","states":[{"at":0,"text":"$40"}]},
  {"id":"store","label":"Authoritative store","detail":"product:42","states":[{"at":0,"text":"$40"},{"at":4,"text":"$45"}]}
]}
```

```json
{"start":0,"end":20,"type":"timeline","title":"Expiration does not track writes","note":"Illustrative clock; 10 seconds is not a default","events":[
  {"time":0,"at":0,"label":"Store copy","value":"$40"},
  {"time":3,"at":4,"label":"Store changes","value":"$45"},
  {"time":5,"at":8,"label":"Stale hit","value":"$40"},
  {"time":10,"at":12,"label":"Copy expires","value":"empty"},
  {"time":11,"at":15,"label":"Next miss","value":"$45"}
]}
```

For queues, transformations, algorithms or spatial problems, author a fitting view using the same `frame` and geometry interface. A queue must show admission, waiting and service; a transformation must show input, operation and changed output. Do not force every topic into cards or claim a template is finished art. Preserve meaningful holds so viewers can read intermediate states.

An optional project `background` names an image inside the source project. The starter centers it without enlargement, shrinking only when needed; smaller textures leave a solid outer margin. Plan that margin as part of the composition. Use a native-resolution surface when the art direction requires full bleed.
