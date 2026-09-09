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
        return <button type="button" key={p.pid} className={`player-row ${p.alive ? '' : 'dead'} ${focus === p.pid ? 'focused' : ''}`} aria-pressed={focus === p.pid} aria-label={`${t('replay.follow')}: ${p.name}${p.alive ? '' : ' · ' + t('common.dead')}`} onClick={() => onFocus(p.pid)}>
          <span className="player-main">
            <span className="player-name"><strong>{p.name}</strong>
              {p.alive && p.defuser && <span className="player-kit" title={t('replay.defuseKit')}>KIT</span>}
              {p.alive && p.bomb && <span className="player-bomb" title={t('replay.hasC4')}>C4</span>}
              {p.alive && p.armor > 0 && <span className="player-armor" title={p.helmet ? t('replay.armorHelmet') : t('replay.armor')} aria-label={`${p.helmet ? t('replay.armorHelmet') : t('replay.armor')}: ${p.armor}`}><i aria-hidden="true" className={p.helmet ? "bi bi-shield-fill app-icon" : "bi bi-shield app-icon"} /></span>}
            </span>
            <span className="player-health"><span className="health-track"><span className={hp <= 30 ? 'health-fill low' : 'health-fill'} style={{ width: `${hp}%` }} /></span><span className="health-value">{hp}</span></span>
            <span className="player-equipment">${p.money}{p.alive ? ` · ${p.weapon}` : ''}</span>
          </span>
          <span className="player-kda"><strong>{p.kills}/{p.deaths}/{p.assists}</strong><small>{t('common.kda')}</small></span>
        </button>;
      })}
    </section>
  );
}
