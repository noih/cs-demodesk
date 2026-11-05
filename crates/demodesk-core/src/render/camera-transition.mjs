const axes = ['x', 'y', 'z', 'rX', 'rY', 'rZ', 'fov'];
const angleDelta = (a, b) => ((b - a + 180) % 360 + 360) % 360 - 180;
const valid = view => view && axes.every(key => Number.isFinite(view[key]));

// Internal starting thresholds; tune against recorded footage, not UI options.
function nearby(from, to) {
  return valid(from) && valid(to)
    && Math.hypot(to.x - from.x, to.y - from.y, to.z - from.z) <= 200
    && ['rX', 'rY', 'rZ'].every(key => Math.abs(angleDelta(from[key], to[key])) <= 60)
    && Math.abs(to.fov - from.fov) <= 25;
}

// shortcut: proximity cannot detect walls; callers must reject known obstructed paths.
// Create once per shot boundary; sample with live endpoints and elapsed seconds.
export function createCameraTransition(from, to, { continuous = true, pathBlocked = false } = {}) {
  let cut = !continuous || pathBlocked || !nearby(from, to);
  let lastElapsed = 0;
  return (source, destination, elapsed) => {
    // Latch a cut if playback seeks or a moving target escapes the safe range.
    if (!Number.isFinite(elapsed) || elapsed < lastElapsed || !nearby(source, destination)) cut = true;
    lastElapsed = elapsed;
    if (cut || elapsed >= 0.75) return destination;
    const t = Math.max(0, elapsed / 0.75);
    const blend = t * t * (3 - 2 * t);
    return Object.fromEntries(axes.map(key => {
      const delta = key.startsWith('r') ? angleDelta(source[key], destination[key]) : destination[key] - source[key];
      return [key, source[key] + delta * blend];
    }));
  };
}
