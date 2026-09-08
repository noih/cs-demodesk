import { Badge, Card, Flex, Grid, Heading, Table, Text } from '@radix-ui/themes';
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
      <Table.Root className="nowrap-headers" size="1" layout="fixed">
        <Table.Header>
          <Table.Row>
            <Table.ColumnHeaderCell>{t('common.player')}</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right" width="40px">K</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right" width="40px">D</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right" width="40px">A</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right" width="52px">K/D</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right" width="52px">HS%</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right" width="64px">{t('common.damage')}</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right" width="56px">ADR</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right" width="72px">{t('common.utilityDamage')}</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right" width="84px">{t('common.multiKills')}</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right" width="60px">{t('common.clutch')}</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right" width="48px">{t('common.highlights')}</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right" width="60px">{t('players.best')}</Table.ColumnHeaderCell>
          </Table.Row>
        </Table.Header>
        <Table.Body>
          {players.map((p) => (
            <Table.Row key={p.steamid}>
              <Table.Cell>
                <Flex align="center" gap="2" style={{ minWidth: 0 }}>
                  <span className="player-dot" style={{ background: colors.get(p.steamid) }} />
                  <Text truncate style={{ display: 'block', minWidth: 0 }}>
                    {p.name}
                  </Text>
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
      <Grid columns="repeat(auto-fit, minmax(820px, 1fr))" gap="4">
        <TeamTable label={t('players.teamA')} color="blue" score={parsed.score.A} players={a} colors={colors} />
        <TeamTable label={t('players.teamB')} color="orange" score={parsed.score.B} players={b} colors={colors} />
      </Grid>
      <Text size="1" color="gray">
        {t('players.multiKillNote')}
      </Text>
    </Flex>
  );
}
