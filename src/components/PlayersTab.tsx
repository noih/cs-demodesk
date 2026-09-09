import { displayPlayerName } from '../playerName.ts';
import { useState, type ReactNode } from 'react';
import { useAppTheme } from '../AppTheme.tsx';
import { Badge, Button, Card, Flex, Heading, Table, Text, Tooltip } from '@radix-ui/themes';
import { useTranslation } from 'react-i18next';
import type { AimStats, ParsedDemo, PlayerStats } from '../api.ts';
import { playerColors } from '../charts/EChart.tsx';

const GROUPS = ['general', 'aim', 'activity', 'utility', 'opening', 'trades', 'clutches', 'duels'] as const;
const percent = (n: number, d: number) => d ? `${(n / d * 100).toFixed(1)}%` : '—';
const EMPTY_AIM: AimStats = { shots: 0, hits: 0, headHits: 0, headEligibleHits: 0, firstShots: 0, firstHits: 0, sprayShots: 0, sprayHits: 0 };
const WEAPON_GROUPS = ['all', 'rifles', 'awp', 'pistols'] as const;
const average = (n: number, d: number) => d ? (n / d).toFixed(2) : '—';
interface Column { id?: string; label: string; value: (p: PlayerStats) => ReactNode; ratio?: (p: PlayerStats) => number }

export function PlayersTab({ parsed }: { parsed: ParsedDemo }) {
  const { t } = useTranslation();
  const [group, setGroup] = useState<(typeof GROUPS)[number]>('general');
  const [weaponGroup, setWeaponGroup] = useState<(typeof WEAPON_GROUPS)[number]>('all');
  const aim = (p: PlayerStats) => p.aim[weaponGroup] ?? EMPTY_AIM;
  const { colors: theme } = useAppTheme();
  const colors = playerColors(parsed.stats, theme.players.split(','));
  const relative = (value: (p: PlayerStats) => number) => {
    const max = Math.max(1, ...parsed.stats.map(value));
    return (p: PlayerStats) => value(p) / max;
  };
  const columns: Record<typeof group, Column[]> = {
    general: [
      { label: t('common.kda'), value: p => `${p.kills} / ${p.deaths} / ${p.assists}` },
      { label: 'K/D', value: p => p.kd.toFixed(2) },
      { label: 'ADR', value: p => p.adr.toFixed(1), ratio: relative(p => p.adr) },
      { label: 'HS%', value: p => `${p.headshotPct}%`, ratio: p => p.headshotPct / 100 },
      { label: 'KAST', value: p => `${p.kast.toFixed(1)}%`, ratio: p => p.kast / 100 },
      { label: t('players.openingKills'), value: p => p.openingKills },
      { label: t('players.openingDeaths'), value: p => p.openingDeaths },
      { label: t('common.multiKills'), value: p => ['2k','3k','4k','5k'].map(k => p.multiKills[k as keyof typeof p.multiKills]).join(' / ') },
      { label: t('common.clutch'), value: p => p.clutchesWon },
    ],
    aim: [
      { label: t('players.shots'), value: p => aim(p).shots },
      { label: t('players.hitShots'), value: p => aim(p).hits },
      { label: t('players.allAccuracy'), value: p => percent(aim(p).hits, aim(p).shots), ratio: p => aim(p).hits / (aim(p).shots || 1) },
      { label: t('players.headAccuracy'), value: p => percent(aim(p).headHits, aim(p).headEligibleHits), ratio: p => aim(p).headHits / (aim(p).headEligibleHits || 1) },
      { label: t('players.firstAccuracy'), value: p => percent(aim(p).firstHits, aim(p).firstShots), ratio: p => aim(p).firstHits / (aim(p).firstShots || 1) },
      { label: t('players.firstSamples'), value: p => aim(p).firstShots },
      { label: t('players.sprayAccuracy'), value: p => percent(aim(p).sprayHits, aim(p).sprayShots), ratio: p => aim(p).sprayHits / (aim(p).sprayShots || 1) },
      { label: t('players.spraySamples'), value: p => aim(p).sprayShots },
    ],
    activity: [
      { label: t('common.damage'), value: p => p.damage, ratio: relative(p => p.damage) },
      { label: t('players.heDamage'), value: p => p.heDamage, ratio: relative(p => p.heDamage) },
      { label: t('players.fireDamage'), value: p => p.fireDamage, ratio: relative(p => p.fireDamage) },
      { label: t('players.shots'), value: p => p.activity.shots, ratio: relative(p => p.activity.shots) },
      { label: t('players.enemiesFlashed'), value: p => p.activity.enemiesFlashed },
      { label: t('players.survived'), value: p => `${p.roundsSurvived} (${percent(p.roundsSurvived, p.roundsPlayed)})` },
      { label: t('common.friendlyDamage'), value: p => p.friendlyDamage },
      { label: t('common.highlights'), value: p => p.highlights },
      { label: t('players.best'), value: p => p.bestScore.toFixed(1) },
    ],
    utility: [
      { label: t('players.flashes'), value: p => p.activity.flashes },
      { label: t('players.smokes'), value: p => p.activity.smokes },
      { label: 'HE', value: p => p.activity.hes },
      { label: t('players.fires'), value: p => p.activity.fires },
      { label: t('players.flashAssists'), value: p => p.flashAssists },
      { label: t('players.enemiesPerFlash'), value: p => average(p.activity.enemiesFlashed, p.activity.flashes) },
      { label: t('players.friendsPerFlash'), value: p => average(p.activity.teammatesFlashed, p.activity.flashes) },
      { label: t('players.blindSeconds'), value: p => `${p.activity.enemyBlindSeconds.toFixed(1)} s` },
      { label: t('players.heAverage'), value: p => average(p.heDamage, p.activity.hes) },
      { label: t('common.utilityDamage'), value: p => p.utilityDamage, ratio: relative(p => p.utilityDamage) },
    ],
    opening: [
      { label: t('players.openingKills'), value: p => p.openingKills },
      { label: t('players.openingDeaths'), value: p => p.openingDeaths },
      { label: '+/−', value: p => p.openingKills - p.openingDeaths },
      { label: t('players.attemptRate'), value: p => percent(p.openingKills + p.openingDeaths, p.roundsPlayed), ratio: p => p.roundsPlayed ? (p.openingKills + p.openingDeaths) / p.roundsPlayed : 0 },
      { label: t('players.successRate'), value: p => percent(p.openingKills, p.openingKills + p.openingDeaths), ratio: p => p.openingKills / (p.openingKills + p.openingDeaths || 1) },
    ],
    trades: [
      { label: t('players.tradeKills'), value: p => p.tradeKills },
      { label: t('players.tradedDeaths'), value: p => p.tradedDeaths },
      { label: t('players.tradedRate'), value: p => percent(p.tradedDeaths, p.deaths), ratio: p => p.tradedDeaths / (p.deaths || 1) },
    ],
    clutches: [
      { label: t('players.attempts'), value: p => p.clutches.length },
      { label: t('players.won'), value: p => p.clutchesWon },
      { label: t('players.successRate'), value: p => percent(p.clutchesWon, p.clutches.length), ratio: p => p.clutchesWon / (p.clutches.length || 1) },
      { label: t('players.details'), value: p => p.clutches.length ? <Flex direction="column" gap="1">{p.clutches.map(c => <Text key={c.round} size="1">{t('common.roundN', { n: c.round })} · {c.side === 'CT' ? 'CT' : 'T'} · 1v{c.versus} · {c.kills} K · {t(`players.${c.outcome}`)}</Text>)}</Flex> : '—' },
    ],
    duels: parsed.stats.map(opponent => ({ id: opponent.steamid, label: displayPlayerName(opponent.name), value: p => {
      if (p.steamid === opponent.steamid || p.team === opponent.team) return '—';
      const kills = p.opponents[opponent.steamid] ?? 0;
      const deaths = opponent.opponents[p.steamid] ?? 0;
      return `${kills} (${percent(kills, kills + deaths)})`;
    }, ratio: p => {
      if (p.team === opponent.team) return 0;
      const kills = p.opponents[opponent.steamid] ?? 0;
      const deaths = opponent.opponents[p.steamid] ?? 0;
      return kills / (kills + deaths || 1);
    } })),
  };
  const playerWidth = Math.max(24, ...parsed.stats.map(p => Array.from(displayPlayerName(p.name)).reduce((n, char) => n + (/[^\x00-\x7f]/.test(char) ? 2 : 1), 0) + 4));
  const columnWidths = columns[group].map(c => Math.max(12, Array.from(c.label).reduce((n, char) => n + (/[^\x00-\x7f]/.test(char) ? 2 : 1), 0) + 4));
  return <Flex direction="column" gap="3">
    <Flex gap="2" wrap="wrap" role="group" aria-label={t('players.category')}>
      {GROUPS.map(key => <Button key={key} type="button" size="1" color={group === key ? 'amber' : 'gray'} variant={group === key ? 'solid' : 'soft'} aria-pressed={group === key} onClick={() => setGroup(key)}>{t(`players.${key}`)}</Button>)}
    </Flex>
    {group === 'aim' && <Flex gap="2" wrap="wrap" role="group" aria-label={t('players.weaponGroup')}>
      {WEAPON_GROUPS.map(key => <Button key={key} size="1" color={weaponGroup === key ? 'amber' : 'gray'} variant={weaponGroup === key ? 'solid' : 'soft'} aria-pressed={weaponGroup === key} onClick={() => setWeaponGroup(key)}>{t(`players.weapon_${key}`)}</Button>)}
    </Flex>}
    {(['A', 'B'] as const).map(team => <Card key={team}>
      <Flex align="center" gap="2" mb="2">
        <Badge color={team === 'A' ? 'blue' : 'orange'}>{t(team === 'A' ? 'players.teamA' : 'players.teamB')}</Badge>
        <Heading data-text-role="subtitle" size="4">{parsed.score[team]}</Heading>
        {group === 'general' && <Text size="1" color="gray">{t('players.kills', { count: parsed.stats.filter(p => p.team === team).reduce((sum, p) => sum + p.kills, 0) })}</Text>}
      </Flex>
      <Table.Root size="1" layout={group === 'clutches' ? 'auto' : 'fixed'} className={`nowrap-headers ${group === 'clutches' ? 'clutch-table' : ''}`} style={{ overflowX: 'auto', minWidth: 0 }}>
        <colgroup><col style={{ width: group === 'clutches' ? '40%' : `${playerWidth}ch` }} />{columnWidths.map((width, i) => <col key={i} style={{ width: group === 'clutches' ? ['20%', '20%', '20%', '1%'][i] : `${width}ch` }} />)}</colgroup>
        <Table.Header><Table.Row>
          <Table.ColumnHeaderCell style={{ minWidth: 140 }}>{t('common.player')}</Table.ColumnHeaderCell>
          {columns[group].map(c => <Table.ColumnHeaderCell key={c.id ?? c.label} align="right">{c.label}</Table.ColumnHeaderCell>)}
        </Table.Row></Table.Header>
        <Table.Body>{parsed.stats.filter(p => p.team === team).map(p => <Table.Row key={p.steamid} className="row-hover">
          <Table.RowHeaderCell><Flex align="center" gap="2"><span className="player-dot" style={{ background: colors.get(p.steamid) }} /><Tooltip delayDuration={150} content={displayPlayerName(p.name)}><Text>{displayPlayerName(p.name)}</Text></Tooltip></Flex></Table.RowHeaderCell>
          {columns[group].map(c => <Table.Cell key={c.id ?? c.label} align="right" style={{ whiteSpace: 'nowrap', backgroundOrigin: 'content-box', backgroundClip: 'content-box', backgroundRepeat: 'no-repeat', backgroundImage: c.ratio ? `linear-gradient(to right, var(--accent-a3) ${Math.max(0, Math.min(1, c.ratio(p))) * 100}%, transparent 0)` : undefined }}>{c.value(p)}</Table.Cell>)}
        </Table.Row>)}</Table.Body>
      </Table.Root>
    </Card>)}
    <Text size="1" color="gray">{t(`players.${group}Note`)} {t('players.barNote')}</Text>
  </Flex>;
}
