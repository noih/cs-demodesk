// Route-follow opening. shortcut: eye-route clearance is not world collision detection.
export function startOpening({ steamId, startTick, endTick, tickRate = 64 }) {
  const id = 'demodesk/opening-experiment';
  const path = [];
  let lastTick, viewYaw;
  mirv.events.cViewRenderSetupView.on(id, () => {
    const tick = mirv.getDemoTick();
    // Keep the eye override across the asynchronous spectator-mode handoff.
    if (tick >= endTick + tickRate * 0.125) { mirv.events.cViewRenderSetupView.off(id); return; }
    if (!mirv.isPlayingDemo()) return;
    let pawn;
    for (let i = 1; i <= 64; i++) {
      const controller = mirv.getEntityFromIndex(i);
      if (!controller?.isPlayerController() || controller.getSteamId().toString() !== steamId) continue;
      const handle = controller.getPlayerPawnHandle();
      if (mirv.isHandleValid(handle)) pawn = mirv.getEntityFromIndex(mirv.getHandleEntryIndex(handle));
      break;
    }
    if (!pawn?.isValid() || !pawn.isPlayerPawn() || pawn.getHealth() <= 0) return;
    const eye = pawn.getRenderEyeOrigin(), angles = pawn.getRenderEyeAngles();
    if (![...eye, ...angles].every(Number.isFinite)) return;
    if (tick >= endTick) return { x: eye[0], y: eye[1], z: eye[2], rX: angles[0], rY: angles[1], rZ: 0 };
    // Collect during seek preroll, so the opening starts on the actual approach route.
    const previous = path.at(-1);
    if (!previous || Math.hypot(...eye.map((v, i) => v - previous[i])) > 1) path.push([...eye]);
    if (path.length > 1024) path.shift();
    if (tick < startTick) return;
    // 1.2 seconds follow, then 0.6 seconds approach including the handoff.
    const turn = Math.min(1, Math.max(0, (tick - startTick - tickRate * 1.2) / Math.max(1, endTick - startTick - tickRate * 1.2 - 1)));
    // Accelerate into the POV at the cut instead of easing to a stop before it.
    const blend = turn * turn * turn;
    let remaining = 70 * (1 - blend);
    let position = [...eye];
    for (let i = path.length - 1; i >= 0; i--) {
      const distance = Math.hypot(...position.map((v, axis) => v - path[i][axis]));
      if (distance >= remaining && distance > 0) {
        position = position.map((v, axis) => v + (path[i][axis] - v) * remaining / distance);
        remaining = 0;
        break;
      }
      remaining -= distance;
      position = path[i];
    }
    // With no approach history, keep a modest shoulder offset until movement supplies it.
    if (remaining > 0) {
      const yaw = angles[1] * Math.PI / 180;
      position = [position[0] - remaining * Math.cos(yaw), position[1] - remaining * Math.sin(yaw), position[2]];
    }
    const [x, y, baseZ] = position;
    const z = baseZ + 16 * (1 - blend);
    const targetYaw = Math.atan2(eye[1] - y, eye[0] - x) * 180 / Math.PI;
    const delta = ((targetYaw - (viewYaw ?? targetYaw) + 540) % 360) - 180;
    viewYaw = (viewYaw ?? targetYaw) + delta * (1 - Math.exp(-Math.max(0, tick - (lastTick ?? tick)) / 8));
    lastTick = tick;
    // Converge along the recorded route before matching the first-person view.
    const yawDelta = ((angles[1] - viewYaw + 540) % 360) - 180;
    const routePitch = Math.atan2(z - eye[2], Math.hypot(eye[0] - x, eye[1] - y)) * 180 / Math.PI;
    return { x, y, z, rX: routePitch + (angles[0] - routePitch) * blend, rY: viewYaw + yawDelta * blend, rZ: 0 };
  });
}
