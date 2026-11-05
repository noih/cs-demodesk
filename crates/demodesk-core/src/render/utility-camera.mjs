// HLAE view override for utility shots. Ends before the next scheduled POV.
// shortcut: no world collision query; validate new map geometry before adding wider offsets.
export function startAimFollow({ steamId, startTick, endTick, grenade }) {
  const id = 'demodesk/editing-aim-follow';
  let controller = null;
  let reported = false;
  let flight = null;
  let lastView = null;
  mirv.events.cViewRenderSetupView.on(id, (event) => {
    const tick = mirv.getDemoTick();
    if (tick >= endTick) {
      mirv.events.cViewRenderSetupView.off(id);
      mirv.message('DEMODESK_AIM_FOLLOW_END\n');
      return;
    }
    if (tick < startTick || !mirv.isPlayingDemo()) return;
    if (grenade && tick >= grenade.throwTick) {
      const projectile = mirv.getEntityFromIndex(grenade.entityIndex);
      if (projectile?.isValid() && projectile.getClassName() === (grenade.className ?? 'flashbang_projectile')) {
        const position = projectile.getOrigin();
        if (position.every(Number.isFinite) && lastView) {
          if (!flight) {
            flight = { view: lastView, time: event.curTime, yaw: lastView.rY * Math.PI / 180 };
            mirv.message('DEMODESK_GRENADE_FOLLOW_ACTIVE\n');
          }
          // Keep the throw direction across bounces; rotating with each bounce is disorienting.
          const target = { x: position[0] - 56 * Math.cos(flight.yaw),
            y: position[1] - 56 * Math.sin(flight.yaw), z: position[2] + 18,
            rX: Math.atan2(18, 56) * 180 / Math.PI, rY: flight.view.rY, rZ: 0 };
          const t = Math.min(1, Math.max(0, (event.curTime - flight.time) / 0.2));
          const blend = t * t * (3 - 2 * t);
          lastView = Object.fromEntries(Object.entries(target).map(([key, value]) =>
            [key, flight.view[key] + (value - flight.view[key]) * blend]));
          return lastView;
        }
      }
      // Hold through detonation until the scheduled cut, without snapping back to the thrower.
      if (flight) return lastView;
    }
    if (!controller?.isValid()) {
      controller = null;
      for (let index = 1; index <= 64; index++) {
        const entity = mirv.getEntityFromIndex(index);
        if (entity?.isPlayerController() && entity.getSteamId().toString() === steamId) {
          controller = entity;
          break;
        }
      }
    }
    if (!controller) return;
    const handle = controller.getPlayerPawnHandle();
    if (!mirv.isHandleValid(handle)) return;
    const pawn = mirv.getEntityFromIndex(mirv.getHandleEntryIndex(handle));
    if (!pawn?.isValid() || !pawn.isPlayerPawn() || pawn.getHealth() <= 0) return;
    const eye = pawn.getRenderEyeOrigin();
    const angles = pawn.getRenderEyeAngles();
    if (![...eye, ...angles].every(Number.isFinite)) return;
    const yaw = angles[1] * Math.PI / 180;
    // Keep the camera behind the shoulder, looking parallel to the player's aim.
    // Horizontal offset avoids sinking into the floor when the player aims up.
    const view = {
      x: eye[0] - 48 * Math.cos(yaw) + 16 * Math.sin(yaw),
      y: eye[1] - 48 * Math.sin(yaw) - 16 * Math.cos(yaw),
      z: eye[2] + 10,
      rX: angles[0], rY: angles[1], rZ: 0,
    };
    if (!reported) {
      reported = true;
      mirv.message('DEMODESK_AIM_FOLLOW_ACTIVE\n');
    }
    lastView = view;
    return view;
  });
}
