import { displayPlayerName } from '../../playerName.ts';
import { Tooltip } from '@radix-ui/themes';
import type { CSSProperties } from 'react';
import { useTranslation } from 'react-i18next';
import type { Team } from '../../api.ts';
import type { PlayerState } from '../../replay/engine.ts';

export function TeamPanel({ side, label, score, players, focus, onFocus }: { side: Team; label: string; score: number; players: PlayerState[]; focus?: number; onFocus: (pid: number) => void }) {
  const { t } = useTranslation();
  const style: CSSProperties & { '--team-color': string } = { '--team-color': side === 'CT' ? 'var(--app-teamA)' : 'var(--app-teamB)' };
  return (
    <section className="team-panel" style={style} aria-label={label}>
      <div className="team-head">
        <div className="team-title"><strong>{label}</strong><small>{side === 'CT' ? t('common.ct') : t('common.t')}</small></div>
        <strong className="team-score">{score}</strong>
      </div>
      {players.map(p => {
        const hp = p.alive ? Math.max(0, Math.min(100, p.hp)) : 0;
        return <Tooltip key={p.pid} delayDuration={150} side="left" content={
          <span className="player-details">
            <strong>{displayPlayerName(p.name)}{!p.alive && ` · ${t('common.dead')}`}</strong>
            <span>{t('replay.toggle.hp')}<b>{hp} / 100</b></span>
            <span>{p.helmet ? t('replay.armorHelmet') : t('replay.armor')}<b>{p.armor}</b></span>
            <span>{t('replay.money')}<b>${p.money}</b></span>
            <span>{t('replay.toggle.weapon')}<b>{p.alive ? p.weapon : '—'}</b></span>
            <span>{t('common.kda')}<b>{p.kills} / {p.deaths} / {p.assists}</b></span>
            {p.alive && p.defuser && <span>{t('replay.defuseKit')}</span>}
            {p.alive && p.bomb && <span>{t('replay.hasC4')}</span>}
          </span>
        }><button type="button" style={{ '--health': `${hp}%` } as CSSProperties} className={`player-row ${p.alive ? '' : 'dead'} ${focus === p.pid ? 'focused' : ''}`} aria-pressed={focus === p.pid} aria-label={`${t('replay.follow')}: ${displayPlayerName(p.name)} · ${hp} HP${p.alive ? '' : ' · ' + t('common.dead')}`} onClick={() => onFocus(p.pid)}>
          <span className="player-main">
            <span className="player-name"><strong>{displayPlayerName(p.name)}</strong>
              {p.alive && p.defuser && <span className="player-kit">KIT</span>}
              {p.alive && p.bomb && <span className="player-bomb">C4</span>}
              {p.alive && p.armor > 0 && <span className="player-armor" aria-label={`${p.helmet ? t('replay.armorHelmet') : t('replay.armor')}: ${p.armor}`}><i aria-hidden="true" className={p.helmet ? "bi bi-shield-fill app-icon" : "bi bi-shield app-icon"} /></span>}
              {p.alive && <span className="player-hp">{hp}</span>}
            </span>
            <span className="player-meta">
              <span className="player-equipment">${p.money}{p.alive ? ` · ${p.weapon}` : ''}</span>
              <span className="player-kda" aria-label={`${t('common.kda')}: ${p.kills}/${p.deaths}/${p.assists}`}>{p.kills}/{p.deaths}/{p.assists}</span>
            </span>
          </span>
        </button></Tooltip>;
      })}
    </section>
  );
}
