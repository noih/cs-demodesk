// The offline planner qualifies the full shoulder path against map geometry.
export function openingView({ startTick, endTick, tickRate = 64, path }, tick, eye, angles) {
  if (tick < startTick || !path || path.length < 2) return;
  if (tick >= endTick) return { x: eye[0], y: eye[1], z: eye[2], rX: angles[0], rY: angles[1], rZ: 0 };
  const next = path.findIndex(p => p.tick >= tick);
  const index = next < 0 ? path.length - 1 : Math.max(1, next);
  const a = path[index - 1], b = path[index];
  const t = Math.min(1, Math.max(0, (tick - a.tick) / (b.tick - a.tick)));
  const turn = Math.min(1, Math.max(0, (tick - startTick - tickRate * 1.2) / Math.max(1, endTick - startTick - tickRate * 1.2 - 1)));
  const blend = turn ** 3;
  const position = a.position.map((v, i) => {
    const base = v + (b.position[i] - v) * t;
    return base + (eye[i] - base) * blend;
  });
  const target = a.target.map((v, i) => v + (b.target[i] - v) * t);
  const delta = target.map((v, i) => v - position[i]);
  const aimYaw = Math.atan2(delta[1], delta[0]) * 180 / Math.PI;
  const aimPitch = -Math.atan2(delta[2], Math.hypot(delta[0], delta[1])) * 180 / Math.PI;
  const yawDelta = ((angles[1] - aimYaw + 540) % 360) - 180;
  return { x: position[0], y: position[1], z: position[2], rX: aimPitch + (angles[0] - aimPitch) * blend, rY: aimYaw + yawDelta * blend, rZ: 0 };
}

export function startOpening(config) {
  const { steamId, endTick, tickRate = 64 } = config;
  const id = 'demodesk/opening-experiment';
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
    return openingView(config, tick, eye, angles);
  });
}
