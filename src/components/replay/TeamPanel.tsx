import { IconButton, Text } from '@radix-ui/themes';
import { EyeOpenIcon } from '@radix-ui/react-icons';
import type { Team } from '../../api.ts';
import type { PlayerState } from '../../replay/engine.ts';
import { teamColor } from '../../replay/draw.ts';

/** One team: header with score, one row per player (HP ring, armor, name, money, K/D/A, focus). */
export function TeamPanel({ side, label, score, players, focus, onFocus }: { side: Team; label: string; score: number; players: PlayerState[]; focus?: number; onFocus: (pid: number) => void }) {
  const color = teamColor(side);
  return (
    <div className="team-panel" style={{ borderTopColor: color }}>
      <div className="team-head">
        <div>
          <Text size="2" weight="bold">
            {label}
          </Text>
          <Text as="div" size="1" style={{ color }}>
            {side === 'CT' ? 'Counter-Terrorists' : 'Terrorists'}
          </Text>
        </div>
        <Text size="6" weight="bold" style={{ color }}>
          {score}
        </Text>
      </div>
      {players.map((p) => (
        <div key={p.pid} className={`player-row ${p.alive ? '' : 'dead'} ${focus === p.pid ? 'focused' : ''}`} onClick={() => onFocus(p.pid)}>
          <HpRing p={p} color={color} />
          <div className="player-main">
            <Text size="2" weight="medium" truncate>
              {p.name}
            </Text>
            <Text size="1" color="gray" truncate>
              ${p.money}
              {p.alive ? ` · ${p.weapon}` : ''}
            </Text>
          </div>
          <div className="player-kda">
            {p.alive ? (
              <Text size="2">
                {p.kills} / {p.deaths} / {p.assists}
              </Text>
            ) : (
              <Text size="2" color="red">
                Dead
              </Text>
            )}
            <Text size="1" color="gray">
              {p.alive ? 'K / D / A' : `${p.kills} / ${p.deaths} / ${p.assists}`}
            </Text>
          </div>
          <IconButton
            size="1"
            variant={focus === p.pid ? 'solid' : 'ghost'}
            color="gray"
            aria-label="鏡頭跟隨"
            onClick={(e) => {
              e.stopPropagation();
              onFocus(p.pid);
            }}
          >
            <EyeOpenIcon />
          </IconButton>
        </div>
      ))}
    </div>
  );
}

function HpRing({ p, color }: { p: PlayerState; color: string }) {
  const r = 14;
  const c = 2 * Math.PI * r;
  const hp = Math.max(0, Math.min(100, p.hp));
  return (
    <div className="hp-ring">
      <svg width="36" height="36" viewBox="0 0 36 36">
        <circle cx="18" cy="18" r={r} fill="none" stroke="var(--gray-a6)" strokeWidth="2.5" />
        {p.alive && <circle cx="18" cy="18" r={r} fill="none" stroke={color} strokeWidth="2.5" strokeDasharray={`${(c * hp) / 100} ${c}`} strokeLinecap="round" transform="rotate(-90 18 18)" />}
        <text x="18" y="19" textAnchor="middle" dominantBaseline="middle" fontSize={p.alive ? 11 : 12} fill={p.alive ? 'var(--gray-12)' : 'var(--gray-a9)'} fontWeight="600">
          {p.alive ? hp : '☠'}
        </text>
      </svg>
      {p.alive && p.armor > 0 && <span className={`armor ${p.helmet ? 'helmet' : ''}`} title={p.helmet ? '護甲 + 頭盔' : '護甲'} />}
      {p.alive && p.bomb && (
        <span className="c4-badge" title="持有 C4">
          C4
        </span>
      )}
      {p.alive && p.defuser && (
        <span className="defuser-badge" title="拆彈器">
          KIT
        </span>
      )}
    </div>
  );
}
