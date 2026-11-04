import type { ReplayData, ReplayFrame } from '../api.ts';

const fields = ['p', 'g', 'd', 'f'] as const;
export interface PackedReplay {
  meta: Omit<ReplayData, 'frames' | 'smoke'>;
  frames: Omit<ReplayFrame, 'p' | 'g' | 'd' | 'f'>[];
  smoke?: {t: number}[];
  rows: Float64Array;
}

/** Transfer numeric rows in one buffer instead of cloning hundreds of thousands of arrays. */
export function packReplay(data: ReplayData): PackedReplay {
  const size = (rows: number[][] | null | undefined) => 1 + (rows?.reduce((n, row) => n + 1 + row.length, 0) ?? 0);
  let length = 0;
  for (const frame of data.frames) for (const field of fields) length += size(frame[field]);
  for (const frame of data.smoke ?? []) length += size(frame.cells);
  const rows = new Float64Array(length);
  let offset = 0;
  const write = (group: number[][] | null | undefined) => {
    rows[offset++] = group == null ? -1 : group.length;
    for (const row of group ?? []) {
      rows[offset++] = row.length;
      rows.set(row, offset);
      offset += row.length;
    }
  };
  const {frames: sourceFrames, smoke: sourceSmoke, ...meta} = data;
  const frames = sourceFrames.map(({p, g, d, f, ...frame}) => {
    write(p); write(g); write(d); write(f);
    return frame;
  });
  const smoke = sourceSmoke?.map(({cells, ...frame}) => { write(cells); return frame; });
  return {meta, frames, smoke, rows};
}

export function unpackReplay(packed: PackedReplay): ReplayData {
  let offset = 0;
  const read = (): number[][] | undefined => {
    const count = packed.rows[offset++]!;
    if (count === -1) return undefined;
    if (!Number.isSafeInteger(count) || count < 0 || count > packed.rows.length - offset) throw new Error('Invalid replay group length');
    const group = new Array<number[]>(count);
    for (let i = 0; i < count; i++) {
      const length = packed.rows[offset++]!;
      if (!Number.isSafeInteger(length) || length < 0 || length > packed.rows.length - offset) throw new Error('Invalid replay row length');
      const row = new Array<number>(length);
      for (let j = 0; j < length; j++) row[j] = packed.rows[offset++]!;
      group[i] = row;
    }
    return group;
  };
  const frames = packed.frames.map(meta => {
    const frame: ReplayFrame = {...meta, p: read()!};
    for (const field of ['g', 'd', 'f'] as const) {
      const rows = read();
      if (rows !== undefined) frame[field] = rows;
    }
    return frame;
  });
  const data: ReplayData = {...packed.meta, frames};
  if (packed.smoke) data.smoke = packed.smoke.map(frame => ({...frame, cells: read() ?? null}));
  if (offset !== packed.rows.length) throw new Error('Unexpected replay rows');
  return data;
}
