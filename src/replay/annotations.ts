import type { Layout } from './draw.ts';

export type DrawingTool = 'pen' | 'arrow' | 'ellipse' | 'rectangle';
export interface Annotation {
  tool: DrawingTool;
  color: string;
  layer: number;
  points: Array<[number, number]>;
}

export function annotationPoint(lay: Layout, x: number, y: number, layer?: number) {
  const index = layer ?? lay.origins.findIndex(([ox, oy]) => x >= ox && y >= oy && x <= ox + lay.side && y <= oy + lay.side);
  const origin = lay.origins[index];
  if (!origin) return;
  const point: [number, number] = [
    Math.max(0, Math.min(1024, (x - origin[0]) / lay.k)),
    Math.max(0, Math.min(1024, (y - origin[1]) / lay.k)),
  ];
  return { layer: index, point };
}

export function drawAnnotations(ctx: CanvasRenderingContext2D, lay: Layout, strokes: Annotation[]) {
  ctx.save();
  ctx.lineCap = 'round';
  ctx.lineJoin = 'round';
  for (const stroke of strokes) {
    const origin = lay.origins[stroke.layer];
    const first = stroke.points[0];
    if (!origin || !first) continue;
    ctx.save();
    ctx.beginPath();
    ctx.rect(origin[0], origin[1], lay.side, lay.side);
    ctx.clip();
    const [x, y] = [origin[0] + first[0] * lay.k, origin[1] + first[1] * lay.k];
    ctx.beginPath();
    const last = stroke.points.at(-1)!;
    const ex = origin[0] + last[0] * lay.k;
    const ey = origin[1] + last[1] * lay.k;
    if (stroke.tool === 'ellipse') ctx.ellipse((x + ex) / 2, (y + ey) / 2, Math.abs(ex - x) / 2, Math.abs(ey - y) / 2, 0, 0, Math.PI * 2);
    else if (stroke.tool === 'rectangle') ctx.rect(Math.min(x, ex), Math.min(y, ey), Math.abs(ex - x), Math.abs(ey - y));
    else {
      ctx.moveTo(x, y);
      for (const p of stroke.points) ctx.lineTo(origin[0] + p[0] * lay.k, origin[1] + p[1] * lay.k);
    }
    if (stroke.tool === 'arrow' && Math.hypot(last[0] - first[0], last[1] - first[1]) * lay.k > 2) {
      const angle = Math.atan2(ey - y, ex - x);
      const head = Math.min(14, Math.hypot(ex - x, ey - y) / 2);
      for (const offset of [-Math.PI / 6, Math.PI / 6]) {
        ctx.moveTo(ex, ey);
        ctx.lineTo(ex - head * Math.cos(angle + offset), ey - head * Math.sin(angle + offset));
      }
    }
    ctx.strokeStyle = '#111';
    ctx.lineWidth = 6;
    ctx.stroke();
    ctx.strokeStyle = stroke.color;
    ctx.lineWidth = 3;
    ctx.stroke();
    ctx.restore();
  }
  ctx.restore();
}
