import { displayPlayerName } from '../playerName.ts';
// Canvas drawing for the 2D replay. One radar image per vertical layer, laid out
// side by side; every marker lands on the layer its z belongs to.
import { GRENADE_KINDS, type MapAssets } from '../api.ts';
import { layerOf, SECONDS, toImage, type TickState } from './engine.ts';
import type { Team } from '../api.ts';

export interface DrawToggles {
  names: boolean;
  hp: boolean;
  weapon: boolean;
  view: boolean;
  grenades: boolean;
  shots: boolean;
  bomb: boolean;
  deaths: boolean;
  /** HTML overlays (not drawn here, but one settings object) */
  killFeed: boolean;
  clock: boolean;
  /** hearing radius (footsteps while running, gunshots): everyone, only the focused player, or nobody */
  sound: 'all' | 'focus' | 'off';
}
export const DEFAULT_TOGGLES: DrawToggles = { names: true, hp: true, weapon: false, view: true, grenades: true, shots: true, bomb: true, deaths: true, killFeed: true, clock: true, sound: 'focus' };

// Sound ranges in world units — approximations of how far CS2 lets you hear
// footsteps and gunfire, not measured values.
const FOOTSTEP_RANGE = 1100;
const SHOT_RANGE = 2400;
const SILENCED_SHOT_RANGE = 1200;
/** below this ground speed (u/s) nobody hears you: shift-walk is ~130 */
const RUN_SPEED = 140;
const SILENCED = ['USP-S', 'M4A1-S'];

/** User zoom / pan on top of the fit-to-canvas layout. */
export interface View {
  zoom: number;
  panX: number;
  panY: number;
}

export const COLORS = {
  ct: '#5fa8ff',
  t: '#f2b134',
  dead: 'rgba(255,255,255,0.45)',
  smoke: 'rgba(200,205,215,0.55)',
  fire: 'rgba(255,120,30,0.5)',
  flash: 'rgba(255,255,255,0.9)',
  he: 'rgba(255,80,80,0.8)',
  decoy: 'rgba(120,220,120,0.6)',
  bomb: '#ff4d4d',
  text: '#e8eaee',
  shadow: 'rgba(0,0,0,0.7)',
};

export const teamColor = (team: Team): string => (team === 'CT' ? COLORS.ct : COLORS.t);

const IMG = 1024;
const GAP = 12;

export interface Layout {
  /** square side of one layer in canvas px */
  side: number;
  /** canvas px per image px */
  k: number;
  /** top-left of each layer */
  origins: Array<[number, number]>;
}

export function layout(width: number, height: number, layers: number, view: View): Layout {
  const n = Math.max(1, layers);
  const horizontal = width / n >= height * 0.75 || width >= height;
  const cols = horizontal ? n : 1;
  const rows = horizontal ? 1 : n;
  const side = Math.max(1, Math.min((width - GAP * (cols - 1)) / cols, (height - GAP * (rows - 1)) / rows)) * view.zoom;
  const totalW = side * cols + GAP * (cols - 1);
  const totalH = side * rows + GAP * (rows - 1);
  const x0 = (width - totalW) / 2 + view.panX;
  const y0 = (height - totalH) / 2 + view.panY;
  const origins: Array<[number, number]> = [];
  for (let i = 0; i < n; i++) origins.push(horizontal ? [x0 + i * (side + GAP), y0] : [x0, y0 + i * (side + GAP)]);
  return { side, k: side / IMG, origins };
}

function place(m: MapAssets, lay: Layout, x: number, y: number, z: number): [number, number, number] {
  const li = layerOf(m, z);
  const [ix, iy] = toImage(m, x, y);
  const [ox, oy] = lay.origins[li] ?? lay.origins[0]!;
  return [ox + ix * lay.k, oy + iy * lay.k, li];
}

export function draw(ctx: CanvasRenderingContext2D, width: number, height: number, m: MapAssets, images: HTMLImageElement[], state: TickState, tg: DrawToggles, view: View, focus?: number, labelSize = 14) {
  ctx.clearRect(0, 0, width, height);
  const lay = layout(width, height, m.layers.length, view);
  const k = lay.k;
  // radar images
  images.forEach((img, i) => {
    const [ox, oy] = lay.origins[i]!;
    if (img.complete && img.naturalWidth > 0) ctx.drawImage(img, ox, oy, lay.side, lay.side);
    if (m.layers.length > 1) {
      ctx.fillStyle = 'rgba(0,0,0,0.5)';
      ctx.font = `${Math.max(10, 12 * Math.sqrt(view.zoom))}px system-ui, sans-serif`;
      const label = m.layers[i]!.name;
      const w = ctx.measureText(label).width + 10;
      ctx.fillRect(ox + 6, oy + 6, w, 18);
      ctx.fillStyle = COLORS.text;
      ctx.textBaseline = 'middle';
      ctx.textAlign = 'left';
      ctx.fillText(label, ox + 11, oy + 15);
    }
  });
  const r = Math.max(4, 7 * k * 1.2);
  const font = `${Math.max(9, Math.round(11 * Math.sqrt(view.zoom)))}px system-ui, sans-serif`;

  // area effects under everything else
  if (tg.grenades) {
    for (const e of state.effects) {
      const [x, y] = place(m, lay, e.x, e.y, e.z);
      const age = (state.tick - e.start) / (e.end - e.start);
      ctx.beginPath();
      switch (e.kind) {
        case 'smoke':
          ctx.fillStyle = COLORS.smoke;
          ctx.arc(x, y, 144 / m.scale * k, 0, Math.PI * 2);
          ctx.fill();
          break;
        case 'fire':
          if (state.fireCells !== undefined) break;
          ctx.fillStyle = COLORS.fire;
          ctx.arc(x, y, 150 / m.scale * k, 0, Math.PI * 2);
          ctx.fill();
          break;
        case 'decoy':
          ctx.strokeStyle = COLORS.decoy;
          ctx.lineWidth = 1.5;
          ctx.arc(x, y, 40 / m.scale * k + 2, 0, Math.PI * 2);
          ctx.stroke();
          break;
        case 'flash':
        case 'he': {
          ctx.strokeStyle = e.kind === 'flash' ? COLORS.flash : COLORS.he;
          ctx.globalAlpha = 1 - age;
          ctx.lineWidth = 3;
          ctx.arc(x, y, (60 + 200 * age) / m.scale * k, 0, Math.PI * 2);
          ctx.stroke();
          ctx.globalAlpha = 1;
          break;
        }
      }
    }
  }

  if (tg.grenades && state.fireCells) {
    ctx.fillStyle = COLORS.fire;
    ctx.beginPath();
    // Half the default 42-unit flame spacing visualizes each recorded cell.
    // These footprints approximate coverage, not the game's damage boundary.
    const cellRadius = 21 / m.scale * k;
    for (const [wx, wy, wz] of state.fireCells) {
      const [x, y] = place(m, lay, wx!, wy!, wz!);
      ctx.moveTo(x + cellRadius, y);
      ctx.arc(x, y, cellRadius, 0, Math.PI * 2);
    }
    ctx.fill();
  }

  // hearing radius: who is making noise right now, and how far it carries
  if (tg.sound !== 'off') {
    const heard = (pid: number) => tg.sound === 'all' || pid === focus;
    for (const p of state.players) {
      if (!heard(p.pid) || !p.alive || p.walking || p.ducking || p.speed < RUN_SPEED) continue;
      const [x, y] = place(m, lay, p.x, p.y, p.z);
      // light translucent disc with a thin rim, like the in-game "hearing radius" overlays
      ctx.beginPath();
      ctx.arc(x, y, (FOOTSTEP_RANGE / m.scale) * k, 0, Math.PI * 2);
      ctx.fillStyle = 'rgba(255,255,255,0.045)';
      ctx.fill();
      ctx.strokeStyle = 'rgba(255,255,255,0.35)';
      ctx.lineWidth = 1;
      ctx.stroke();
    }
    for (const sh of state.shots) {
      if (!heard(sh.pid)) continue;
      const [x, y] = place(m, lay, sh.x, sh.y, sh.z);
      const range = SILENCED.includes(sh.weapon) ? SILENCED_SHOT_RANGE : SHOT_RANGE;
      ctx.beginPath();
      ctx.arc(x, y, (range / m.scale) * k * (0.6 + 0.4 * sh.age), 0, Math.PI * 2);
      ctx.strokeStyle = '#fff';
      ctx.globalAlpha = Math.max(0, 0.5 - sh.age * 0.5);
      ctx.lineWidth = 1.5;
      ctx.stroke();
      ctx.globalAlpha = 1;
    }
  }

  // death spots
  if (tg.deaths) {
    ctx.strokeStyle = COLORS.dead;
    ctx.lineWidth = 2;
    for (const d of state.deaths) {
      const [x, y] = place(m, lay, d.x, d.y, d.z);
      const s = r * 0.8;
      ctx.beginPath();
      ctx.moveTo(x - s, y - s);
      ctx.lineTo(x + s, y + s);
      ctx.moveTo(x + s, y - s);
      ctx.lineTo(x - s, y + s);
      ctx.stroke();
    }
  }

  // bomb on the ground
  if (tg.bomb && state.bomb && state.bomb.x !== undefined && state.bomb.state !== 'carried') {
    const b = state.bomb;
    const [x, y] = place(m, lay, b.x!, b.y!, b.z ?? 0);
    const pulse = b.state === 'planted' ? 0.5 + 0.5 * Math.abs(Math.sin(state.tick / 8)) : 1;
    ctx.fillStyle = b.state === 'exploded' ? COLORS.he : b.state === 'defused' ? COLORS.ct : COLORS.bomb;
    ctx.globalAlpha = pulse;
    ctx.fillRect(x - r * 0.7, y - r * 0.7, r * 1.4, r * 1.4);
    ctx.globalAlpha = 1;
    ctx.fillStyle = '#000';
    ctx.font = `bold ${Math.max(8, r * 1.1)}px system-ui, sans-serif`;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText('C4', x, y + 0.5);
    if (b.state === 'planted' && b.remaining !== undefined) {
      // timer bar + seconds under the marker; rows spaced by the text height so
      // the two numbers (timer / defuse) never overlap
      const w = r * 6;
      const h = Math.max(4, r * 0.45);
      const rowH = Math.max(h + 2, 12 * Math.sqrt(view.zoom) + 1);
      const bx = x - w / 2;
      const by = y + r * 1.1;
      ctx.font = font;
      ctx.textAlign = 'left';
      ctx.textBaseline = 'middle';
      const row = (top: number, color: string, frac: number, text: string) => {
        const cy = top + rowH / 2;
        ctx.fillStyle = 'rgba(0,0,0,0.6)';
        ctx.fillRect(bx, cy - h / 2, w, h);
        ctx.fillStyle = color;
        ctx.fillRect(bx, cy - h / 2, w * frac, h);
        ctx.lineWidth = 3;
        ctx.strokeStyle = COLORS.shadow;
        ctx.strokeText(text, bx + w + 4, cy);
        ctx.fillStyle = COLORS.text;
        ctx.fillText(text, bx + w + 4, cy);
      };
      row(by, COLORS.bomb, b.remaining / SECONDS.bomb, b.remaining.toFixed(1));
      if (b.defuse) row(by + rowH, COLORS.ct, b.defuse.remaining / b.defuse.needs, b.defuse.remaining.toFixed(1));
    }
  }

  // shots
  if (tg.shots) {
    ctx.lineWidth = 1.5;
    for (const s of state.shots) {
      const [x, y] = place(m, lay, s.x, s.y, s.z);
      const a = (-s.yaw * Math.PI) / 180;
      const len = 700 / m.scale * k;
      ctx.globalAlpha = Math.max(0, 1 - s.age);
      ctx.strokeStyle = '#fff';
      ctx.beginPath();
      ctx.moveTo(x, y);
      ctx.lineTo(x + Math.cos(a) * len, y + Math.sin(a) * len);
      ctx.stroke();
    }
    ctx.globalAlpha = 1;
  }

  // grenades in flight
  if (tg.grenades) {
    for (const g of state.grenades) {
      const [x, y] = place(m, lay, g.x, g.y, g.z);
      const kind = GRENADE_KINDS[g.kind] ?? 'he';
      ctx.fillStyle = kind === 'smoke' ? '#9ca3af' : kind === 'flash' ? '#60a5fa' : kind === 'he' ? '#ff6b6b' : kind === 'molotov' ? '#ff9a3c' : '#8fe38f';
      ctx.beginPath();
      ctx.arc(x, y, Math.max(2.5, r * 0.45), 0, Math.PI * 2);
      ctx.fill();
      ctx.strokeStyle = '#000';
      ctx.lineWidth = 1;
      ctx.stroke();
    }
  }

  // Labels stay in screen pixels while marker positions follow the map scale.
  ctx.font = `${labelSize}px system-ui, sans-serif`;
  const orderedPlayers = [...state.players].sort((a, b) => Number(a.pid === focus) - Number(b.pid === focus));
  for (const p of orderedPlayers) {
    if (!p.alive) continue;
    const [x, y] = place(m, lay, p.x, p.y, p.z);
    const color = p.pid === focus ? '#b995ff' : teamColor(p.team);
    // Fill health from the bottom over a dark backing that contrasts with the map.
    ctx.save();
    ctx.beginPath();
    ctx.arc(x, y, r, 0, Math.PI * 2);
    ctx.clip();
    ctx.fillStyle = 'rgba(0,0,0,0.55)';
    ctx.fillRect(x - r, y - r, r * 2, r * 2);
    ctx.fillStyle = color;
    const fill = tg.hp ? Math.max(0, Math.min(100, p.hp)) / 100 : 1;
    ctx.fillRect(x - r, y + r - r * 2 * fill, r * 2, r * 2 * fill);
    ctx.restore();
    ctx.beginPath();
    ctx.arc(x, y, r, 0, Math.PI * 2);
    ctx.lineWidth = p.blind ? 3 : 1.5;
    ctx.strokeStyle = p.blind ? '#fff' : 'rgba(0,0,0,0.8)';
    ctx.stroke();
    if (tg.view) {
      // facing tick on the disc
      const a = (-p.yaw * Math.PI) / 180;
      ctx.strokeStyle = '#000';
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.moveTo(x + Math.cos(a) * r * 0.3, y + Math.sin(a) * r * 0.3);
      ctx.lineTo(x + Math.cos(a) * r * 1.5, y + Math.sin(a) * r * 1.5);
      ctx.stroke();
    }
    if (tg.bomb && p.bomb) {
      // carrier: red dot in the middle of the disc
      ctx.fillStyle = COLORS.bomb;
      ctx.beginPath();
      ctx.arc(x, y, r * 0.42, 0, Math.PI * 2);
      ctx.fill();
      ctx.strokeStyle = 'rgba(0,0,0,0.8)';
      ctx.lineWidth = 1;
      ctx.stroke();
    }
  }
  // Labels are drawn above every player marker; the focused label is last.
  for (const p of orderedPlayers) {
    if (!p.alive) continue;
    const [x, y] = place(m, lay, p.x, p.y, p.z);
    const lines: string[] = [];
    if (tg.names) lines.push(displayPlayerName(p.name));
    if (tg.weapon && p.weapon) lines.push(p.weapon);
    if (lines.length) {
      // small dark label above the disc, like the in-game spectator tag
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      const lh = labelSize + 2;
      const w = Math.max(...lines.map((t) => ctx.measureText(t).width)) + 8;
      const h = lh * lines.length + 3;
      const top = y - r - 4 - h;
      ctx.fillStyle = 'rgba(0,0,0,0.6)';
      ctx.fillRect(x - w / 2, top, w, h);
      lines.forEach((text, i) => {
        ctx.fillStyle = i === 0 && tg.names ? COLORS.text : 'rgba(232,234,238,0.75)';
        ctx.fillText(text, x, top + 2 + lh * i + lh / 2);
      });
    }
  }
}
