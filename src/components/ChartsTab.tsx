import { displayPlayerName } from '../playerName.ts';
import { useAppTheme } from '../AppTheme.tsx';
import type { AppColors } from '../themes.ts';
import { useMemo, useState } from 'react';
import { Box, Button, Card, Flex, Grid, Heading, Select, Text, Tooltip } from '@radix-ui/themes';
import type { EChartsCoreOption } from 'echarts/core';
import { useTranslation } from 'react-i18next';
import type { TFunction } from 'i18next';
import { EChart, playerColors } from '../charts/EChart.tsx';
import { RecoilChart } from './RecoilChart.tsx';
import { TRENDS, trendSeries, type Trend } from '../charts/trends.ts';
import type { ParsedDemo, PlayerStats } from '../api.ts';

/** Multi-kill rounds weighted by size (2k = 1 … 5k = 4). */
const multiScore = (p: PlayerStats) => p.multiKills['2k'] + p.multiKills['3k'] * 2 + p.multiKills['4k'] * 3 + p.multiKills['5k'] * 4;

function roundTimelineOption(parsed: ParsedDemo, metric: Trend, t: TFunction, colors: AppColors, fontSize: number): EChartsCoreOption {
  const palette = playerColors(parsed.stats, colors.players.split(','));
  const teamMetric = metric === 'cash' || metric === 'difference';
  const series = trendSeries(parsed, metric).map(s => ({
    id: s.id,
    name: teamMetric ? s.id === 'difference' ? t('charts.trend.difference') : t(s.id === 'B' ? 'common.teamB' : 'common.teamA') : displayPlayerName(parsed.stats.find(p => p.steamid === s.id)?.name),
    type: 'line', data: s.values, showSymbol: false, symbolSize: 4, connectNulls: false,
    lineStyle: { width: 2 },
    itemStyle: { color: teamMetric ? s.id === 'B' ? colors.teamB : colors.teamA : palette.get(s.id) },
  }));
  return {
    tooltip: { trigger: 'axis', order: 'valueDesc', renderMode: 'richText', axisPointer: { type: 'line' } },
    legend: { show: false },
    grid: { left: fontSize * 4, right: 25, top: 25, bottom: fontSize * 3 + 12 },
    xAxis: { type: 'category', boundaryGap: false, data: parsed.roundSummaries.map(r => String(r.round)), axisLabel: { color: colors.muted }, name: t('charts.roundAxis'), nameLocation: 'middle', nameGap: fontSize + 16 },
    yAxis: { type: 'value', minInterval: 1, axisLabel: { color: colors.muted }, splitLine: { lineStyle: { color: colors.border } } },
    series,
  };
}

/** Player comparison: grouped bars (K / D / A / HS%) or a radar per player. */
const METRICS = ['kills', 'kd', 'hs', 'damage', 'adr', 'utility', 'multi', 'clutch', 'best'] as const;
type Metric = (typeof METRICS)[number];

function playerBarsOption(parsed: ParsedDemo, metric: Metric, t: TFunction, colors: AppColors, fontSize: number): EChartsCoreOption {
  const players = playerColors(parsed.stats, colors.players.split(','));
  const value = (p: ParsedDemo['stats'][number]) => {
    switch (metric) {
      case 'kd':
        return p.kd;
      case 'hs':
        return p.headshotPct;
      case 'damage':
        return p.damage;
      case 'adr':
        return p.adr;
      case 'utility':
        return p.utilityDamage;
      case 'multi':
        return multiScore(p);
      case 'clutch':
        return p.clutchesWon;
      case 'best':
        return p.bestScore;
      default:
        return p.kills;
    }
  };
  const stats = [...parsed.stats].sort((a, b) => value(b) - value(a));
  return {
    tooltip: { trigger: 'axis', axisPointer: { type: 'shadow' } },
    grid: { left: fontSize * 8, right: fontSize * 4, top: 10, bottom: fontSize * 3 + 12 },
    xAxis: { type: 'value', name: t(`charts.metric.${metric}`), nameLocation: 'middle', nameGap: fontSize + 16, splitLine: { lineStyle: { color: colors.border } }, axisLabel: { color: colors.muted }, nameTextStyle: { color: colors.muted } },
    yAxis: { type: 'category', inverse: true, data: stats.map((p) => displayPlayerName(p.name)), axisLabel: { color: colors.text, width: fontSize * 7, overflow: 'truncate' }, axisLine: { lineStyle: { color: colors.border } } },
    series: [
      {
        type: 'bar',
        data: stats.map((p) => ({ value: value(p), itemStyle: { color: players.get(p.steamid) } })),
        label: { show: true, position: 'right', color: colors.text, formatter: (p: { value: number }) => (Number.isInteger(p.value) ? String(p.value) : p.value.toFixed(2)) },
        barWidth: '55%',
      },
    ],
  };
}

function playerRadarOption(parsed: ParsedDemo, steamids: string[], t: TFunction, colors: AppColors, fontSize: number): EChartsCoreOption {
  const players = playerColors(parsed.stats, colors.players.split(','));
  const max = { adr: Math.max(1, ...parsed.stats.map((p) => p.adr)), kills: Math.max(1, ...parsed.stats.map((p) => p.kills)), kd: Math.max(1, ...parsed.stats.map((p) => p.kd)), hs: 100, multi: Math.max(1, ...parsed.stats.map((p) => multiScore(p))), clutch: Math.max(1, ...parsed.stats.map((p) => p.clutchesWon)), best: Math.max(1, ...parsed.stats.map((p) => p.bestScore)) };
  const picked = parsed.stats.filter((p) => steamids.includes(p.steamid));
  return {
    tooltip: {},
    legend: { data: picked.map((p) => displayPlayerName(p.name)), textStyle: { color: colors.muted }, bottom: 0 },
    radar: {
      radius: '58%',
      axisNameGap: fontSize,
      indicator: [
        { name: t('charts.radar.kills'), max: max.kills },
        { name: 'ADR', max: max.adr },
        { name: 'K/D', max: max.kd },
        { name: t('charts.radar.hs'), max: max.hs },
        { name: t('charts.radar.multi'), max: max.multi },
        { name: t('common.clutch'), max: max.clutch },
        { name: t('charts.radar.best'), max: max.best },
      ],
      splitLine: { lineStyle: { color: colors.border } },
      splitArea: { show: false },
      axisName: { color: colors.muted },
    },
    series: [
      {
        type: 'radar',
        data: picked.map((p) => ({
          name: displayPlayerName(p.name),
          value: [p.kills, p.adr, p.kd, p.headshotPct, multiScore(p), p.clutchesWon, p.bestScore],
          lineStyle: { color: players.get(p.steamid) },
          itemStyle: { color: players.get(p.steamid) },
          areaStyle: { opacity: 0.12 },
        })),
      },
    ],
  };
}

export function ChartsTab({ parsed }: { parsed: ParsedDemo }) {
  const { t } = useTranslation();
  const { colors, typography } = useAppTheme();
  const [trend, setTrend] = useState<Trend>('kills');
  const [hiddenSeries, setHiddenSeries] = useState<string[]>([]);
  const [metric, setMetric] = useState<Metric>('kills');
  const [radarA, setRadarA] = useState(parsed.stats[0]?.steamid ?? '');
  const [radarB, setRadarB] = useState(parsed.stats.find((p) => p.team === 'B')?.steamid ?? parsed.stats[1]?.steamid ?? '');
  const timeline = useMemo(() => roundTimelineOption(parsed, trend, t, colors, typography[3]!), [parsed, trend, t, colors, typography]);
  const bars = useMemo(() => playerBarsOption(parsed, metric, t, colors, typography[3]!), [parsed, metric, t, colors, typography]);
  const radar = useMemo(() => playerRadarOption(parsed, [radarA, radarB].filter(Boolean), t, colors, typography[3]!), [parsed, radarA, radarB, t, colors, typography]);

  return (
    <Flex direction="column" gap="4">
      <Card>
        <Heading data-text-role="subtitle" size="3" mb="1">
          {t('charts.timelineTitle')}
        </Heading>
        <Text size="1" color="gray" as="p" mb="2">
          {t('charts.timelineHint')}
        </Text>
        <Flex gap="1" wrap="wrap" mb="3">
          {TRENDS.map(key => <Button key={key} size="1" color={key === trend ? 'amber' : 'gray'} variant={key === trend ? 'solid' : 'soft'} aria-pressed={key === trend} onClick={() => setTrend(key)}>{t(`charts.trend.${key}`)}</Button>)}
        </Flex>
        <Box mb="2" style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(36px, 1fr))', gap: 4 }}>
          {parsed.roundSummaries.map(r => {
            const highlights = parsed.highlights.filter(h => h.round === r.round);
            const detail = [t('common.roundN', { n: r.round }), r.winner ? t('charts.roundTooltipWin', { team: t(r.winner === 'A' ? 'common.teamA' : 'common.teamB') }) : t('common.unknown'), t('charts.roundKills', { a: r.killsA, b: r.killsB }), ...highlights.map(h => h.title)].join(' · ');
            return <Tooltip delayDuration={150} key={r.round} content={detail}><Box className="round-summary" tabIndex={0} aria-label={detail} style={{ textAlign: 'center', borderTop: '4px solid ' + (r.winner === 'A' ? colors.teamA : r.winner === 'B' ? colors.teamB : colors.muted), background: colors.panel, padding: '10px 4px', minWidth: 36, cursor: 'help' }}>
              <Text as="div" size="1" style={{ fontVariantNumeric: 'tabular-nums', whiteSpace: 'nowrap' }}>{r.round}</Text>
              <Text as="div" size="1" aria-hidden="true" style={{ minHeight: '1.5em' }}>{highlights.length > 0 ? '★' : '\u00a0'}</Text>
            </Box></Tooltip>;
            })}
        </Box>
        <Text size="1" color="gray">{t(trend === 'cash' ? 'charts.cashHint' : trend === 'difference' ? 'charts.differenceHint' : 'charts.cumulativeHint')}</Text>
        <EChart option={{ ...timeline, series: (timeline.series as Array<{ id: string }>).filter(series => !hiddenSeries.includes(series.id)) }} height={380} />
        <div className="chart-legend" aria-label={t('charts.timelineTitle')}>
          {(timeline.series as Array<{ id: string; name: string; itemStyle: { color: string } }>).map(series => <button type="button" key={series.id} aria-pressed={!hiddenSeries.includes(series.id)} onClick={() => setHiddenSeries(current => current.includes(series.id) ? current.filter(name => name !== series.id) : [...current, series.id])}>
            <span aria-hidden="true" style={{ background: series.itemStyle.color }} />{series.name}
          </button>)}
        </div>
      </Card>
      <RecoilChart parsed={parsed} />
      <Grid columns={{ initial: '1', lg: 'minmax(0, 3fr) minmax(0, 2fr)' }} gap="4" align="start">
        <Card style={{ minWidth: 0 }}>
          {/* pills that wrap: nine labels never fit one segmented row in every language */}
          <Flex gap="1" wrap="wrap" mb="2">
            {METRICS.map((m) => (
              <Button key={m} size="1" variant={m === metric ? 'solid' : 'soft'} color={m === metric ? undefined : 'gray'} aria-pressed={m === metric} onClick={() => setMetric(m)}>
                {t(`charts.seg.${m}`)}
              </Button>
            ))}
          </Flex>
          <EChart option={bars} height={Math.max(260, parsed.stats.length * 30 + 60)} />
        </Card>
        <Card style={{ minWidth: 0 }}>
          <Flex direction="column" mb="2" gap="2">
            <Heading data-text-role="subtitle" size="3">{t('charts.radarTitle')}</Heading>
            <Flex gap="2" style={{ width: '100%' }}>
              {[
                [radarA, setRadarA],
                [radarB, setRadarB],
              ].map(([v, set], i) => (
                <Select.Root key={i} value={v as string} onValueChange={set as (v: string) => void}>
                  <Select.Trigger className="bounded-select" style={{ flex: 1, width: 0 }} title={displayPlayerName(parsed.stats.find(p => p.steamid === v)?.name)} />
                  <Select.Content>
                    {parsed.stats.map((p) => (
                      <Select.Item key={p.steamid} value={p.steamid}>
                        {displayPlayerName(p.name)}
                      </Select.Item>
                    ))}
                  </Select.Content>
                </Select.Root>
              ))}
            </Flex>
          </Flex>
          <Box>
            <EChart option={radar} height={Math.max(320, parsed.stats.length * 30 + 60)} />
          </Box>
        </Card>
      </Grid>
    </Flex>
  );
}
