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
  const plots = WEAPONS.map(([id, label]) => {
    const points = player?.recoil?.[id] ?? [];
    const reference = parsed.recoilReference?.[id] ?? [];
    const minX = Math.min(0, ...reference.map(p => p.x)), maxX = Math.max(0, ...reference.map(p => p.x));
    const minY = Math.min(0, ...reference.map(p => p.y)), maxY = Math.max(0, ...reference.map(p => p.y));
    return { id, label, points, reference, cx: (minX + maxX) / 2, cy: (minY + maxY) / 2 };
  });
  const all = plots.flatMap(p => [...p.points, ...p.reference]);
  // Centre each reference, while sharing the angular scale across weapons.
  const halfSpan = Math.max(3, ...plots.flatMap(p => (p.reference.length ? p.reference : p.points)
    .map(v => Math.max(Math.abs(v.x - p.cx), Math.abs(v.y - p.cy)) * 1.2))) / (zoom * 0.8);
  const playerColor = colors.players.split(',')[7]!;
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
      {plots.map(({ id, label, points, reference, cx, cy }) => {
        const clipped = points.filter(p => Math.abs(p.x - cx) > halfSpan || Math.abs(p.y - cy) > halfSpan).length;
        return <Card key={id} style={{ minWidth: 0 }}>
          <Flex justify="between" align="center" gap="2">
            <Heading size="2">{label}</Heading>
            <Text size="1" color="gray">{t('recoil.bursts', { n: points[0]?.samples ?? 0 })}</Text>
          </Flex>
          <Flex gap="3" wrap="wrap" mt="2" mb="1">
            <Text size="1" style={{ color: playerColor }}>━ {t('recoil.you')}</Text>
            {reference.length > 0 && <Text size="1" style={{ color: colors.accent }}>━ {t('recoil.reference')}</Text>}
          </Flex>
          {points.length === 0 && reference.length === 0 ? <Flex align="center" justify="center" style={{ minHeight: 180 }}><Text size="2" color="gray">{t('recoil.empty')}</Text></Flex> : <>
            <Box role="img" aria-label={`${player?.name} · ${label} · ${t('recoil.title')}`} style={{ aspectRatio: '1', width: '100%', maxWidth: 420, margin: 'auto' }}>
              <EChart height="100%" option={{
                animation: false,
                tooltip: { trigger: 'item', renderMode: 'richText', formatter: (param: { dataIndex: number; seriesIndex: number }) => `${param.seriesIndex === 0 ? t('recoil.reference') : t('recoil.you')}\n${pointDescription((param.seriesIndex === 0 ? reference : points)[param.dataIndex]!, param.dataIndex)}` },
                grid: { left: 45, right: 20, top: 20, bottom: 45 },
                xAxis: { type: 'value', min: cx - halfSpan, max: cx + halfSpan, name: t('recoil.horizontal'), nameLocation: 'middle', nameGap: 28, axisLabel: { color: colors.muted, formatter: (value: number) => String(Number(value.toFixed(1))) }, splitLine: { lineStyle: { color: colors.border } } },
                yAxis: { type: 'value', min: cy - halfSpan, max: cy + halfSpan, axisLabel: { color: colors.muted, formatter: (value: number) => String(Number(value.toFixed(1))) }, splitLine: { lineStyle: { color: colors.border } } },
                series: [reference, points].map((path, index) => ({ type: 'line', z: index + 2, data: path.map(p => [p.x, p.y]), symbol: 'circle', symbolSize: index === 0 ? 5 : 4, showSymbol: true,
                  lineStyle: { width: index === 0 ? 2 : 1.5, color: index === 0 ? colors.accent : playerColor }, itemStyle: { color: index === 0 ? colors.accent : playerColor }, labelLayout: { hideOverlap: true },
                  label: { show: true, position: 'right', color: colors.text, fontSize: 10, formatter: (p: { dataIndex: number }) => p.dataIndex === 0 || (p.dataIndex + 1) % 5 === 0 ? String(p.dataIndex + 1) : '' },
                })),
              }} />
            </Box>
            <Text as="p" size="1" color="gray" mb="2">{t('recoil.vertical')}</Text>
            {reference.length === 0 && <Text as="p" size="1" color="gray">{t('recoil.noReference')}</Text>}
            {clipped > 0 && <Text as="p" size="1" color="amber">{t('recoil.clipped', { n: clipped })}</Text>}
            {points.length === 0 && <Text as="p" size="1" color="gray">{t('recoil.empty')}</Text>}
            <details>
              <summary style={{ cursor: 'pointer' }}>{t('recoil.details')}</summary>
              <Table.Root size="1" aria-label={`${label} · ${t('recoil.details')}`}>
                <Table.Header><Table.Row>
                  <Table.ColumnHeaderCell>{t('recoil.shot')}</Table.ColumnHeaderCell><Table.ColumnHeaderCell>X°</Table.ColumnHeaderCell><Table.ColumnHeaderCell>Y°</Table.ColumnHeaderCell><Table.ColumnHeaderCell>n</Table.ColumnHeaderCell><Table.ColumnHeaderCell>{t('recoil.reference')} X° / Y° / n</Table.ColumnHeaderCell>
                </Table.Row></Table.Header>
                <Table.Body>{Array.from({ length: Math.max(points.length, reference.length) }, (_, i) => <Table.Row key={i}>
                  <Table.RowHeaderCell>{i + 1}</Table.RowHeaderCell><Table.Cell>{points[i]?.x.toFixed(2) ?? '—'}</Table.Cell><Table.Cell>{points[i]?.y.toFixed(2) ?? '—'}</Table.Cell><Table.Cell>{points[i]?.samples ?? '—'}</Table.Cell>
                  <Table.Cell>{reference[i] ? `${reference[i].x.toFixed(2)} / ${reference[i].y.toFixed(2)} / ${reference[i].samples}` : '—'}</Table.Cell>
                </Table.Row>)}</Table.Body>
              </Table.Root>
            </details>
          </>}
        </Card>;
      })}
    </Grid>
    <Text as="p" size="1" color="gray" mt="3">{t('recoil.definition')}</Text>
  </Card>;
}
