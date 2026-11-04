import { displayPlayerName } from '../../playerName.ts';
import { useMemo, useRef } from 'react';
import type { RoundInfo } from '../../api.ts';
import type { Replay } from '../../replay/engine.ts';
import { teamColor } from '../../replay/draw.ts';

/** Timeline of the current round: kills as ticks, bomb plant as a red segment, click / drag to seek. */
export function RoundTimeline({ replay, round, tick, onSeek }: { replay: Replay; round?: RoundInfo; tick: number; onSeek: (t: number) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const nextRoundAt = useRef(0);
  const start = round?.startTick ?? replay.firstTick;
  const end = round?.officiallyEndedTick ?? replay.lastTick;
  const span = Math.max(1, end - start);
  const pct = (t: number) => `${Math.min(100, Math.max(0, ((t - start) / span) * 100))}%`;
  const kills = useMemo(() => (round ? replay.killsIn(round) : []), [replay, round]);
  const plant = useMemo(() => {
    if (!round) return undefined;
    const p = replay.data.events.find((e) => e.k === 'plant' && e.t >= round.startTick && e.t <= round.endTick);
    if (!p) return undefined;
    const done = replay.data.events.find((e) => (e.k === 'defuse' || e.k === 'explode') && e.t > p.t && e.t <= round.officiallyEndedTick);
    return { from: p.t, to: done?.t ?? round.endTick };
  }, [replay, round]);
  const seekAt = (clientX: number) => {
    const rect = ref.current!.getBoundingClientRect();
    if (rect.width <= 0) return;
    const position = Math.min(1, Math.max(0, (clientX - rect.left) / rect.width));
    const index = round ? replay.parsed.rounds.indexOf(round) : -1;
    const next = index >= 0 ? replay.parsed.rounds[index + 1] : undefined;
    if (position === 1 && next) {
      const now = performance.now();
      if (now < nextRoundAt.current) return;
      // Pointer events keep arriving against the new round while the drag stays at the edge.
      nextRoundAt.current = now + 600;
      onSeek(next.startTick);
      return;
    }
    const seekEnd = next ? Math.min(end, next.startTick - 1) : end;
    onSeek(start + position * Math.max(0, seekEnd - start));
  };
  return (
    <div
      ref={ref}
      className="round-timeline"
      onPointerDown={(e) => {
        e.currentTarget.setPointerCapture(e.pointerId);
        seekAt(e.clientX);
      }}
      onPointerMove={(e) => e.buttons === 1 && seekAt(e.clientX)}
    >
      {plant && <div className="tl-plant" style={{ left: pct(plant.from), width: `calc(${pct(plant.to)} - ${pct(plant.from)})` }} />}
      {plant && (
        <div className="tl-bomb" style={{ left: pct(plant.from) }}>
          B
        </div>
      )}
      {round && <div className="tl-mark" style={{ left: pct(round.freezeEndTick), background: 'var(--gray-a8)' }} />}
      {kills.map((k, i) => (
        <div key={i} className="tl-mark" style={{ left: pct(k.tick), background: teamColor(k.attacker?.team ?? k.victim.team) }} title={`${k.attacker ? displayPlayerName(k.attacker.name) : ''} → ${displayPlayerName(k.victim.name)}`} />
      ))}
      <div className="tl-progress" style={{ width: pct(tick) }} />
      <div className="tl-head" style={{ left: pct(tick) }} />
    </div>
  );
}
