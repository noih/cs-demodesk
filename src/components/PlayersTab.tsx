import { Badge, Card, Flex, Grid, Heading, Table, Text, Tooltip } from '@radix-ui/themes';
import { useTranslation } from 'react-i18next';
import type { ParsedDemo, PlayerStats } from '../api.ts';
import { playerColors } from '../charts/EChart.tsx';

function TeamTable({ label, color, score, players, colors }: { label: string; color: 'blue' | 'orange'; score: number; players: PlayerStats[]; colors: Map<string, string> }) {
  const { t } = useTranslation();
  return (
    <Card>
      <Flex align="center" gap="2" mb="2">
        <Badge color={color} size="2">
          {label}
        </Badge>
        <Heading size="4">{score}</Heading>
        <Text size="1" color="gray">
          {t('players.kills', { count: players.reduce((s, p) => s + p.kills, 0) })}
        </Text>
      </Flex>
      <Table.Root className="nowrap-headers" size="1" style={{ overflowX: 'auto' }}>
        <Table.Header>
          <Table.Row>
            <Table.ColumnHeaderCell width="100%" style={{ minWidth: 120 }}>{t('common.player')}</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right">K</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right">D</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right">A</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right">K/D</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right">HS%</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right">{t('common.damage')}</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right">ADR</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right">{t('common.utilityDamage')}</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right">{t('common.multiKills')}</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right">{t('common.clutch')}</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right">{t('common.highlights')}</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right">{t('players.best')}</Table.ColumnHeaderCell>
          </Table.Row>
        </Table.Header>
        <Table.Body>
          {players.map((p) => (
            <Table.Row key={p.steamid} className="row-hover">
              <Table.Cell style={{ maxWidth: 0 }}>
                <Flex align="center" gap="2">
                  <span className="player-dot" style={{ background: colors.get(p.steamid) }} />
                  <Tooltip content={p.name}>
                    <Text truncate style={{ display: 'block', minWidth: 0 }}>
                      {p.name}
                    </Text>
                  </Tooltip>
                </Flex>
              </Table.Cell>
              <Table.Cell align="right">
                <Text weight="bold">{p.kills}</Text>
              </Table.Cell>
              <Table.Cell align="right">{p.deaths}</Table.Cell>
              <Table.Cell align="right">{p.assists}</Table.Cell>
              <Table.Cell align="right">{p.kd.toFixed(2)}</Table.Cell>
              <Table.Cell align="right">{p.headshotPct}%</Table.Cell>
              <Table.Cell align="right">{p.damage}</Table.Cell>
              <Table.Cell align="right">{p.adr.toFixed(1)}</Table.Cell>
              <Table.Cell align="right">{p.utilityDamage}</Table.Cell>
              <Table.Cell align="right" style={{ whiteSpace: 'nowrap' }}>
                <Text size="1" color="gray">
                  {(['2k', '3k', '4k', '5k'] as const).map((k) => p.multiKills[k]).join(' / ')}
                </Text>
              </Table.Cell>
              <Table.Cell align="right">{p.clutchesWon}</Table.Cell>
              <Table.Cell align="right">{p.highlights}</Table.Cell>
              <Table.Cell align="right">
                <Text color={p.bestScore >= 8 ? 'amber' : 'gray'}>{p.bestScore > 0 ? p.bestScore.toFixed(1) : '—'}</Text>
              </Table.Cell>
            </Table.Row>
          ))}
        </Table.Body>
      </Table.Root>
    </Card>
  );
}

export function PlayersTab({ parsed }: { parsed: ParsedDemo }) {
  const { t } = useTranslation();
  const a = parsed.stats.filter((p) => p.team === 'A');
  const b = parsed.stats.filter((p) => p.team === 'B');
  const colors = playerColors(parsed.stats);
  return (
    <Flex direction="column" gap="2">
      {/* two tables side by side only when each gets its full column widths; otherwise stacked, full width */}
      <Grid columns="repeat(auto-fit, minmax(min(760px, 100%), 1fr))" gap="4">
        <TeamTable label={t('players.teamA')} color="blue" score={parsed.score.A} players={a} colors={colors} />
        <TeamTable label={t('players.teamB')} color="orange" score={parsed.score.B} players={b} colors={colors} />
      </Grid>
      <Text size="1" color="gray">
        {t('players.multiKillNote')}
      </Text>
    </Flex>
  );
}
