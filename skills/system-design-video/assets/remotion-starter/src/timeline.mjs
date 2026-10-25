export const frameAt = (seconds, fps) => Math.ceil(seconds * fps - 1e-8);

export function compositionFrames(spec) {
  const count = frameAt(spec.profile.duration_seconds, spec.profile.fps);
  for (const event of spec.events) {
    if ((spec.schema_version === 2 || event.state_after) &&
        frameAt(event.at + (event.duration_seconds ?? 0), spec.profile.fps) >= count) {
      throw new Error(`Event ${event.id} has no visible completion frame; extend the video and final scene with an end hold`);
    }
  }
  return count;
}

export function stateAt(spec, frame) {
  const fps = spec.profile.fps;
  const states = {};
  // Completion order, not start order, determines the latest visible state.
  const completed = spec.events
    .filter((event) => frame >= frameAt(event.at + (event.duration_seconds ?? 0), fps))
    .sort((a, b) => (a.at + (a.duration_seconds ?? 0)) - (b.at + (b.duration_seconds ?? 0)));
  for (const event of completed) Object.assign(states, event.state_after ?? {});
  const packets = spec.events.filter((event) => event.edge_id &&
    frame >= frameAt(event.at, fps) &&
    frame < frameAt(event.at + (event.duration_seconds ?? 0), fps))
    .map((event) => ({...event, progress: Math.max(0, Math.min(1,
      (frame / fps - event.at) / event.duration_seconds))}));
  return {states, packets};
}
