import { useState } from 'react';
import { Box, Button, Card, Flex, Grid, Heading, IconButton, Select, Table, Text } from '@radix-ui/themes';
import { useTranslation } from 'react-i18next';
import { useAppTheme } from '../AppTheme.tsx';
import { EChart } from '../charts/EChart.tsx';
import type { ParsedDemo, RecoilPoint } from '../api.ts';

const WEAPONS = [['ak47', 'AK-47'], ['m4a1', 'M4A4'], ['m4a1_silencer', 'M4A1-S']] as const;

export function RecoilChart({ parsed }: { parsed: ParsedDemo }) {
  const { t } = useTranslation();
  const { colors } = useAppTheme();
  const [selected, setSelected] = useState(parsed.stats.find(p => Object.keys(p.recoil ?? {}).length)?.steamid ?? parsed.stats[0]?.steamid ?? '');
  const [zoom, setZoom] = useState(1);
  const player = parsed.stats.find(p => p.steamid === selected) ?? parsed.stats[0];
  const all = WEAPONS.flatMap(([id]) => player?.recoil?.[id] ?? []);
  // Equal scales, centred on the data rather than wasting half the plot above the origin.
  const minX = Math.min(0, ...all.map(p => p.x)), maxX = Math.max(0, ...all.map(p => p.x));
  const minY = Math.min(0, ...all.map(p => p.y)), maxY = Math.max(0, ...all.map(p => p.y));
  const halfSpan = Math.max(3, Math.max(maxX - minX, maxY - minY) * 0.6) / zoom;
  const centreX = (minX + maxX) / 2, centreY = (minY + maxY) / 2;
  const pointDescription = (point: RecoilPoint, index: number) => t('recoil.point', { shot: index + 1, x: point.x.toFixed(2), y: point.y.toFixed(2), n: point.samples });

  return <Card data-testid="recoil-chart">
    <Flex justify="between" align="center" gap="3" wrap="wrap" mb="2">
      <Heading size="3">{t('recoil.title')}</Heading>
      <Flex align="center" gap="3" wrap="wrap">
        <Flex align="center" gap="2">
          <IconButton size="1" variant="outline" disabled={zoom === 0.25 || !all.length} aria-label={t('replay.zoomOut')} title={t('replay.zoomOut')} onClick={() => setZoom(value => Math.max(0.25, value - 0.25))}>−</IconButton>
          <Button size="1" variant="outline" disabled={!all.length} aria-label={t('recoil.resetZoom')} title={t('recoil.resetZoom')} onClick={() => setZoom(1)} style={{ minWidth: 60, fontVariantNumeric: 'tabular-nums' }}>{zoom * 100}%</Button>
          <IconButton size="1" variant="outline" disabled={zoom === 2 || !all.length} aria-label={t('replay.zoomIn')} title={t('replay.zoomIn')} onClick={() => setZoom(value => Math.min(2, value + 0.25))}>＋</IconButton>
        </Flex>
        {player && <Select.Root value={player.steamid} onValueChange={value => { setSelected(value); setZoom(1); }}>
          <Select.Trigger aria-label={t('recoil.player')} />
          <Select.Content>{parsed.stats.map(p => <Select.Item key={p.steamid} value={p.steamid}>{p.name}</Select.Item>)}</Select.Content>
        </Select.Root>}
      </Flex>
    </Flex>
    <Text as="p" size="1" color="gray" mb="3">{t('recoil.hint')}</Text>
    <Grid columns={{ initial: '1', md: '3' }} gap="3">
      {WEAPONS.map(([id, label]) => {
        const points = player?.recoil?.[id] ?? [];
        return <Card key={id} style={{ minWidth: 0 }}>
          <Flex justify="between" align="center" gap="2">
            <Heading size="2">{label}</Heading>
            <Text size="1" color="gray">{t('recoil.bursts', { n: points[0]?.samples ?? 0 })}</Text>
          </Flex>
          {points.length === 0 ? <Flex align="center" justify="center" style={{ minHeight: 180 }}><Text size="2" color="gray">{t('recoil.empty')}</Text></Flex> : <>
            <Box role="img" aria-label={`${player?.name} · ${label} · ${t('recoil.title')}`} style={{ aspectRatio: '1', width: '100%', maxWidth: 420, margin: 'auto' }}>
              <EChart height="100%" option={{
                animation: false,
                tooltip: { trigger: 'item', renderMode: 'richText', formatter: (param: { dataIndex: number }) => pointDescription(points[param.dataIndex]!, param.dataIndex) },
                grid: { left: 45, right: 20, top: 20, bottom: 45 },
                xAxis: { type: 'value', min: centreX - halfSpan, max: centreX + halfSpan, name: t('recoil.horizontal'), nameLocation: 'middle', nameGap: 28, axisLabel: { color: colors.muted, formatter: (value: number) => String(Number(value.toFixed(1))) }, splitLine: { lineStyle: { color: colors.border } } },
                yAxis: { type: 'value', min: centreY - halfSpan, max: centreY + halfSpan, axisLabel: { color: colors.muted, formatter: (value: number) => String(Number(value.toFixed(1))) }, splitLine: { lineStyle: { color: colors.border } } },
                series: [{ type: 'line', data: points.map(p => [p.x, p.y]), symbol: 'circle', symbolSize: 6, showSymbol: true,
                  lineStyle: { width: 2, color: colors.accent }, itemStyle: { color: colors.accent }, labelLayout: { hideOverlap: true },
                  label: { show: true, position: 'right', color: colors.text, fontSize: 10, formatter: (p: { dataIndex: number }) => p.dataIndex === 0 || (p.dataIndex + 1) % 5 === 0 ? String(p.dataIndex + 1) : '' },
                }],
              }} />
            </Box>
            <Text as="p" size="1" color="gray" mb="2">{t('recoil.vertical')}</Text>
            <details>
              <summary style={{ cursor: 'pointer' }}>{t('recoil.details')}</summary>
              <Table.Root size="1" aria-label={`${label} · ${t('recoil.details')}`}>
                <Table.Header><Table.Row>
                  <Table.ColumnHeaderCell>{t('recoil.shot')}</Table.ColumnHeaderCell><Table.ColumnHeaderCell>X°</Table.ColumnHeaderCell><Table.ColumnHeaderCell>Y°</Table.ColumnHeaderCell><Table.ColumnHeaderCell>n</Table.ColumnHeaderCell>
                </Table.Row></Table.Header>
                <Table.Body>{points.map((p, i) => <Table.Row key={i}><Table.RowHeaderCell>{i + 1}</Table.RowHeaderCell><Table.Cell>{p.x.toFixed(2)}</Table.Cell><Table.Cell>{p.y.toFixed(2)}</Table.Cell><Table.Cell>{p.samples}</Table.Cell></Table.Row>)}</Table.Body>
              </Table.Root>
            </details>
          </>}
        </Card>;
      })}
    </Grid>
    <Text as="p" size="1" color="gray" mt="3">{t('recoil.definition')}</Text>
  </Card>;
}
