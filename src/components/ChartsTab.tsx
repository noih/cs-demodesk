import { useMemo, useState } from 'react';
import { Box, Card, Flex, Grid, Heading, SegmentedControl, Select, Text } from '@radix-ui/themes';
import type { EChartsCoreOption } from 'echarts/core';
import { EChart, CHART_THEME, playerColors } from '../charts/EChart.tsx';
import type { ParsedDemo, PlayerStats } from '../api.ts';

/** Multi-kill rounds weighted by size (2k = 1 … 5k = 4). */
const multiScore = (p: PlayerStats) => p.multiKills['2k'] + p.multiKills['3k'] * 2 + p.multiKills['4k'] * 3 + p.multiKills['5k'] * 4;

/** Round timeline: who won each round, kills per team, and where the highlights sit. */
function roundTimelineOption(parsed: ParsedDemo): EChartsCoreOption {
  const rounds = parsed.roundSummaries;
  const x = rounds.map((r) => String(r.round));
  const winA = rounds.map((r) => (r.winner === 'A' ? 1 : 0));
  const winB = rounds.map((r) => (r.winner === 'B' ? -1 : 0));
  const bestByRound = new Map<number, { score: number; title: string }>();
  for (const h of parsed.highlights) {
    const cur = bestByRound.get(h.round);
    if (!cur || h.score > cur.score) bestByRound.set(h.round, { score: h.score, title: h.title });
  }
  const highlightPoints = rounds.map((r) => {
    const b = bestByRound.get(r.round);
    return b ? [r.round - 1, r.winner === 'B' ? -1.35 : 1.35, b.score, b.title] : null;
  }).filter(Boolean);
  return {
    tooltip: {
      trigger: 'axis',
      axisPointer: { type: 'shadow' },
      formatter: (params: unknown) => {
        const p = params as Array<{ dataIndex: number }>;
        const r = rounds[p[0]?.dataIndex ?? 0];
        if (!r) return '';
        const best = bestByRound.get(r.round);
        return [`<b>第 ${r.round} 回合</b> — ${r.winner ? `Team ${r.winner} 勝` : '未知'}${r.bombPlanted ? ' · 有下包' : ''}`, `擊殺 A ${r.killsA} : B ${r.killsB}`, best ? `最佳高光 ${best.score.toFixed(1)}：${best.title}` : '無高光'].join('<br/>');
      },
    },
    legend: { data: ['Team A 勝', 'Team B 勝', 'A 擊殺', 'B 擊殺'], textStyle: { color: CHART_THEME.muted }, top: 0 },
    grid: { left: 40, right: 20, top: 36, bottom: 30 },
    xAxis: { type: 'category', data: x, axisLine: { lineStyle: { color: CHART_THEME.grid } }, axisLabel: { color: CHART_THEME.muted } },
    yAxis: [
      { type: 'value', min: -1.6, max: 1.6, show: false },
      { type: 'value', name: '擊殺', position: 'right', splitLine: { lineStyle: { color: CHART_THEME.grid } }, axisLabel: { color: CHART_THEME.muted }, nameTextStyle: { color: CHART_THEME.muted } },
    ],
    series: [
      { name: 'Team A 勝', type: 'bar', stack: 'win', data: winA, itemStyle: { color: CHART_THEME.teamA }, barWidth: '60%' },
      { name: 'Team B 勝', type: 'bar', stack: 'win', data: winB, itemStyle: { color: CHART_THEME.teamB }, barWidth: '60%' },
      { name: 'A 擊殺', type: 'line', yAxisIndex: 1, data: rounds.map((r) => r.killsA), lineStyle: { color: CHART_THEME.teamA, width: 1.5 }, itemStyle: { color: CHART_THEME.teamA }, symbolSize: 5 },
      { name: 'B 擊殺', type: 'line', yAxisIndex: 1, data: rounds.map((r) => r.killsB), lineStyle: { color: CHART_THEME.teamB, width: 1.5 }, itemStyle: { color: CHART_THEME.teamB }, symbolSize: 5 },
      {
        name: '高光',
        type: 'scatter',
        data: highlightPoints,
        symbol: 'diamond',
        symbolSize: (v: number[]) => 8 + Math.min(v[2] ?? 0, 15),
        itemStyle: { color: CHART_THEME.accent },
        tooltip: { formatter: (p: unknown) => String((p as { value: unknown[] }).value[3]) },
      },
    ],
  };
}

/** Player comparison: grouped bars (K / D / A / HS%) or a radar per player. */
function playerBarsOption(parsed: ParsedDemo, metric: string): EChartsCoreOption {
  const colors = playerColors(parsed.stats);
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
  const label: Record<string, string> = { kills: '擊殺', kd: 'K/D', hs: '爆頭率 %', damage: '傷害', adr: 'ADR', utility: '道具傷害', multi: '多殺分（2k=1 3k=2 4k=3 ace=4）', clutch: 'Clutch 勝', best: '最高高光分' };
  return {
    tooltip: { trigger: 'axis', axisPointer: { type: 'shadow' } },
    grid: { left: 110, right: 56, top: 10, bottom: 30 },
    xAxis: { type: 'value', name: label[metric], nameLocation: 'middle', nameGap: 22, splitLine: { lineStyle: { color: CHART_THEME.grid } }, axisLabel: { color: CHART_THEME.muted }, nameTextStyle: { color: CHART_THEME.muted } },
    yAxis: { type: 'category', inverse: true, data: stats.map((p) => p.name), axisLabel: { color: CHART_THEME.text, width: 96, overflow: 'truncate' }, axisLine: { lineStyle: { color: CHART_THEME.grid } } },
    series: [
      {
        type: 'bar',
        data: stats.map((p) => ({ value: value(p), itemStyle: { color: colors.get(p.steamid) } })),
        label: { show: true, position: 'right', color: CHART_THEME.text, formatter: (p: { value: number }) => (Number.isInteger(p.value) ? String(p.value) : p.value.toFixed(2)) },
        barWidth: '55%',
      },
    ],
  };
}

function playerRadarOption(parsed: ParsedDemo, steamids: string[]): EChartsCoreOption {
  const colors = playerColors(parsed.stats);
  const max = { adr: Math.max(1, ...parsed.stats.map((p) => p.adr)), kills: Math.max(1, ...parsed.stats.map((p) => p.kills)), kd: Math.max(1, ...parsed.stats.map((p) => p.kd)), hs: 100, multi: Math.max(1, ...parsed.stats.map((p) => multiScore(p))), clutch: Math.max(1, ...parsed.stats.map((p) => p.clutchesWon)), best: Math.max(1, ...parsed.stats.map((p) => p.bestScore)) };
  const picked = parsed.stats.filter((p) => steamids.includes(p.steamid));
  return {
    tooltip: {},
    legend: { data: picked.map((p) => p.name), textStyle: { color: CHART_THEME.muted }, bottom: 0 },
    radar: {
      indicator: [
        { name: '擊殺', max: max.kills },
        { name: 'ADR', max: max.adr },
        { name: 'K/D', max: max.kd },
        { name: '爆頭率', max: max.hs },
        { name: '多殺', max: max.multi },
        { name: 'Clutch', max: max.clutch },
        { name: '高光分', max: max.best },
      ],
      splitLine: { lineStyle: { color: CHART_THEME.grid } },
      splitArea: { show: false },
      axisName: { color: CHART_THEME.muted },
    },
    series: [
      {
        type: 'radar',
        data: picked.map((p) => ({
          name: p.name,
          value: [p.kills, p.adr, p.kd, p.headshotPct, multiScore(p), p.clutchesWon, p.bestScore],
          lineStyle: { color: colors.get(p.steamid) },
          itemStyle: { color: colors.get(p.steamid) },
          areaStyle: { opacity: 0.12 },
        })),
      },
    ],
  };
}

export function ChartsTab({ parsed }: { parsed: ParsedDemo }) {
  const [metric, setMetric] = useState('kills');
  const [radarA, setRadarA] = useState(parsed.stats[0]?.steamid ?? '');
  const [radarB, setRadarB] = useState(parsed.stats.find((p) => p.team === 'B')?.steamid ?? parsed.stats[1]?.steamid ?? '');
  const timeline = useMemo(() => roundTimelineOption(parsed), [parsed]);
  const bars = useMemo(() => playerBarsOption(parsed, metric), [parsed, metric]);
  const radar = useMemo(() => playerRadarOption(parsed, [radarA, radarB].filter(Boolean)), [parsed, radarA, radarB]);

  return (
    <Flex direction="column" gap="4">
      <Card>
        <Heading size="3" mb="1">
          回合時間軸
        </Heading>
        <Text size="1" color="gray" as="p" mb="2">
          上方藍/橘柱為勝方，折線為每回合擊殺數，菱形是該回合最佳高光（越大分越高）。
        </Text>
        <EChart option={timeline} height={300} />
      </Card>
      <Grid columns={{ initial: '1', lg: 'minmax(0, 3fr) minmax(0, 2fr)' }} gap="4" align="start">
        <Card style={{ minWidth: 0 }}>
          <SegmentedControl.Root size="1" value={metric} onValueChange={setMetric} mb="2">
            <SegmentedControl.Item value="kills">擊殺</SegmentedControl.Item>
            <SegmentedControl.Item value="kd">K/D</SegmentedControl.Item>
            <SegmentedControl.Item value="hs">爆頭率</SegmentedControl.Item>
            <SegmentedControl.Item value="damage">傷害</SegmentedControl.Item>
            <SegmentedControl.Item value="adr">ADR</SegmentedControl.Item>
            <SegmentedControl.Item value="utility">道具</SegmentedControl.Item>
            <SegmentedControl.Item value="multi">多殺</SegmentedControl.Item>
            <SegmentedControl.Item value="clutch">Clutch</SegmentedControl.Item>
            <SegmentedControl.Item value="best">高光分</SegmentedControl.Item>
          </SegmentedControl.Root>
          <EChart option={bars} height={Math.max(260, parsed.stats.length * 30 + 60)} />
        </Card>
        <Card style={{ minWidth: 0 }}>
          <Flex justify="between" align="center" mb="2" gap="2" wrap="wrap">
            <Heading size="3">雷達對比</Heading>
            <Flex gap="2">
              {[
                [radarA, setRadarA],
                [radarB, setRadarB],
              ].map(([v, set], i) => (
                <Select.Root key={i} value={v as string} onValueChange={set as (v: string) => void}>
                  <Select.Trigger />
                  <Select.Content>
                    {parsed.stats.map((p) => (
                      <Select.Item key={p.steamid} value={p.steamid}>
                        {p.name}
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
