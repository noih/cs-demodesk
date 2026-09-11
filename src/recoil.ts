import type { RecoilPoint, RecoilShot } from './api.ts';

const CM_PER_UNIT = 2.54;
const PLANE_DISTANCE_CM = 1000;
type Vector = [number, number, number];
const dot = (a: Vector, b: Vector) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
function basis(pitch: number, yaw: number) {
  const p = pitch * Math.PI / 180, y = yaw * Math.PI / 180;
  const forward: Vector = [Math.cos(p) * Math.cos(y), Math.cos(p) * Math.sin(y), -Math.sin(p)];
  const right: Vector = [Math.sin(y), -Math.cos(y), 0];
  const up: Vector = [Math.sin(p) * Math.cos(y), Math.sin(p) * Math.sin(y), Math.cos(p)];
  return { forward, right, up };
}

/** Project view rays with their moving origins onto the first view's fixed 10 m plane. */
function projectRays(shots: { origin: Vector; pitch: number; yaw: number }[], first = shots[0]): (RecoilPoint | null)[] {
  if (!first) return [];
  const { forward, right, up } = basis(first.pitch, first.yaw);
  return shots.map(shot => {
    const direction = basis(shot.pitch, shot.yaw).forward;
    const delta: Vector = [
      (shot.origin[0] - first.origin[0]) * CM_PER_UNIT,
      (shot.origin[1] - first.origin[1]) * CM_PER_UNIT,
      (shot.origin[2] - first.origin[2]) * CM_PER_UNIT,
    ];
    const towardPlane = dot(direction, forward);
    const distance = (PLANE_DISTANCE_CM - dot(delta, forward)) / towardPlane;
    // A ray facing away from the plane or originating beyond it has no forward intersection.
    if (towardPlane <= 1e-6 || distance <= 0 || !Number.isFinite(distance)) return null;
    const x = dot(delta, right) + distance * dot(direction, right);
    const y = dot(delta, up) + distance * dot(direction, up);
    return Number.isFinite(x) && Number.isFinite(y) ? { x: x === 0 ? 0 : x, y: y === 0 ? 0 : y, samples: 1 } : null;
  });
}

/** Remove fixed-target tracking, then project compensation into the standard 10 m frame. */
export function projectShots(shots: RecoilShot[]): (RecoilPoint | null)[] {
  const first = shots[0];
  if (!first) return [];
  const { forward } = basis(first.viewPitch, first.viewYaw);
  const target = first.origin.map((value, i) => value + forward[i]! * PLANE_DISTANCE_CM / CM_PER_UNIT);
  const origin: Vector = [0, 0, 0];
  const rays = shots.map(shot => {
    const dx = target[0]! - shot.origin[0], dy = target[1]! - shot.origin[1], dz = target[2]! - shot.origin[2];
    const targetPitch = -Math.atan2(dz, Math.hypot(dx, dy)) * 180 / Math.PI;
    const targetYaw = Math.atan2(dy, dx) * 180 / Math.PI;
    return { origin, pitch: Math.hypot(dx, dy, dz) > 1e-6 ? shot.viewPitch - targetPitch : NaN,
      yaw: shot.viewYaw - targetYaw };
  });
  return projectRays(rays, { origin, pitch: 0, yaw: 0 });
}

export function averagePaths(paths: (RecoilPoint | null)[][]): (RecoilPoint | null)[] {
  const mean: (RecoilPoint | null)[] = [];
  for (const path of paths) path.forEach((point, i) => {
    if (mean.length <= i) mean.push(null);
    if (!point) return;
    const value = mean[i] ?? { x: 0, y: 0, samples: 0 };
    value.samples += 1;
    value.x += (point.x - value.x) / value.samples;
    value.y += (point.y - value.y) / value.samples;
    mean[i] = value;
  });
  return mean;
}

/** The reference is the eye movement needed to counter the measured recoil angles. */
export function projectReference(points: RecoilPoint[]): (RecoilPoint | null)[] {
  return projectRays(points.map(p => ({ origin: [0, 0, 0], yaw: -p.x, pitch: -p.y })))
    .map((p, i) => p && ({ ...p, samples: points[i]!.samples }));
}

/** Step along 20% marks, with 5% and 250% as the end stops. */
export function stepRecoilZoom(zoom: number, direction: 1 | -1): number {
  const mark = Math.round(zoom * 100) / 20;
  const next = direction > 0 ? Math.floor(mark) + 1 : Math.ceil(mark) - 1;
  return Math.max(5, Math.min(250, next * 20)) / 100;
}
