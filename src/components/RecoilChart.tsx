import { displayPlayerName } from '../playerName.ts';
import { useEffect, useRef, useState } from 'react';
import { Box, Button, Card, Flex, Grid, Heading, IconButton, Select, Text } from '@radix-ui/themes';
import { useTranslation } from 'react-i18next';
import { useAppTheme } from '../AppTheme.tsx';
import { EChart } from '../charts/EChart.tsx';
import type { ParsedDemo, RecoilPoint } from '../api.ts';

import calibration from '../data/recoil-reference.json';

const SHOT_INTERVAL_MS = 100;
const WEAPONS = [['ak47', 'AK-47'], ['m4a1', 'M4A4'], ['m4a1_silencer', 'M4A1-S']] as const;

export function RecoilChart({ parsed }: { parsed: ParsedDemo }) {
  const { t } = useTranslation();
  const { colors, typography } = useAppTheme();
  const [selected, setSelected] = useState(parsed.stats.find(p => Object.keys(p.recoil ?? {}).length)?.steamid ?? parsed.stats[0]?.steamid ?? '');
  const [visible, setVisible] = useState({ player: true, reference: true });
  const [playback, setPlayback] = useState<Record<string, { elapsed: number; total: number; startedAt: number | null }>>({});
  const isPlaying = Object.values(playback).some(state => state.startedAt !== null);
  useEffect(() => { setPlayback({}); }, [parsed, selected]);
  useEffect(() => {
    if (!isPlaying) return;
    // Timestamps keep shot timing independent of delayed interval callbacks.
    const timer = window.setInterval(() => {
      const now = performance.now();
      setPlayback(current => Object.fromEntries(
        Object.entries(current).flatMap(([id, state]) => {
          if (state.startedAt === null) return [[id, state]];
          const elapsed = now - state.startedAt;
          return elapsed < state.total * SHOT_INTERVAL_MS ? [[id, { ...state, elapsed }]] : [];
        }),
      ));
    }, 50);
    return () => window.clearInterval(timer);
  }, [isPlaying]);
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const root = useRef<HTMLDivElement>(null);
  const drag = useRef<{ id: string; x: number; y: number } | undefined>(undefined);
  useEffect(() => {
    const element = root.current!;
    const wheel = (event: WheelEvent) => {
      if (!(event.target instanceof Element) || !event.target.closest('.recoil-plot')) return;
      event.preventDefault();
      if (event.deltaY) setZoom(value => Math.max(0.25, Math.min(2, value + (event.deltaY < 0 ? 0.25 : -0.25))));
    };
    element.addEventListener('wheel', wheel, { passive: false });
    return () => element.removeEventListener('wheel', wheel);
  }, []);
  const player = parsed.stats.find(p => p.steamid === selected) ?? parsed.stats[0];
  const plots = WEAPONS.map(([id, label]) => {
    const points = player?.recoil?.[id] ?? [];
    const reference = calibration.weapons[id];
    const minX = Math.min(0, ...reference.map(p => p.x)), maxX = Math.max(0, ...reference.map(p => p.x));
    const minY = Math.min(0, ...reference.map(p => p.y)), maxY = Math.max(0, ...reference.map(p => p.y));
    return { id, label, points, reference, cx: (minX + maxX) / 2, cy: (minY + maxY) / 2 };
  });
  const all = plots.flatMap(p => [...p.points, ...p.reference]);
  // Centre each reference, while sharing the angular scale across weapons.
  const halfSpan = Math.max(3, ...plots.flatMap(p => (p.reference.length ? p.reference : p.points)
    .map(v => Math.max(Math.abs(v.x - p.cx), Math.abs(v.y - p.cy)) * 1.2))) / (zoom * 0.8);
  const inset = Math.ceil(typography[3]! * 3 + 8);
  const playerColor = colors.players.split(',')[7]!;
  const pointDescription = (point: RecoilPoint, index: number) => t('recoil.point', { shot: index + 1, x: point.x.toFixed(2), y: point.y.toFixed(2), n: point.samples });

  return <Card ref={root} data-testid="recoil-chart">
    <Flex justify="between" align="center" gap="3" wrap="wrap" mb="2">
      <Heading data-text-role="subtitle" size="3">{t('recoil.title')}</Heading>
      <Flex className="recoil-controls" align="center" gap="3" wrap="wrap">
        <Flex align="center" gap="2">
          <IconButton size="1" variant="outline" disabled={zoom === 0.25 || !all.length} aria-label={t('replay.zoomOut')} title={t('replay.zoomOut')} onClick={() => setZoom(value => Math.max(0.25, value - 0.25))}>−</IconButton>
          <Button size="1" variant="outline" disabled={!all.length} aria-label={t('recoil.resetZoom')} title={t('recoil.resetZoom')} onClick={() => { setZoom(1); setPan({ x: 0, y: 0 }); }} style={{ minWidth: 60, fontVariantNumeric: 'tabular-nums' }}>{zoom * 100}%</Button>
          <IconButton size="1" variant="outline" disabled={zoom === 2 || !all.length} aria-label={t('replay.zoomIn')} title={t('replay.zoomIn')} onClick={() => setZoom(value => Math.min(2, value + 0.25))}>＋</IconButton>
        </Flex>
        {player && <Select.Root value={player.steamid} onValueChange={value => { setSelected(value); setZoom(1); setPan({ x: 0, y: 0 }); }}>
          <Select.Trigger aria-label={t('recoil.player')} />
          <Select.Content>{parsed.stats.map(p => <Select.Item key={p.steamid} value={p.steamid}>{displayPlayerName(p.name)}</Select.Item>)}</Select.Content>
        </Select.Root>}
      </Flex>
    </Flex>
    <Grid style={{ gridTemplateColumns: 'repeat(auto-fit, minmax(min(100%, 360px), 1fr))' }} gap="3">
      {plots.map(({ id, label, points, reference, cx: referenceX, cy: referenceY }) => {
        const cx = referenceX + pan.x, cy = referenceY + pan.y;
        const state = playback[id];
        const playing = !!state && state.startedAt !== null;
        const shown = state ? Math.min(points.length, Math.floor(state.elapsed / SHOT_INTERVAL_MS) + 1) : points.length;
        const pulse = playing && state.elapsed % SHOT_INTERVAL_MS < SHOT_INTERVAL_MS / 2;
        const playbackLabel = `${t(playing ? 'replay.pause' : 'replay.play')} ${label}`;
        return <Card key={id} style={{ minWidth: 0 }}>
          <Flex justify="between" align="center" gap="2">
            <Heading data-text-role="body-heading" size="2">{label}</Heading>
            <Flex align="center" gap="2">
              <Text size="1" color="gray">{t('recoil.bursts', { n: points[0]?.samples ?? 0 })}</Text>
              <IconButton size="1" variant="ghost" disabled={!points.length} aria-label={playbackLabel} title={playbackLabel}
                onClick={() => {
                  setVisible(current => ({ ...current, player: true }));
                  const now = performance.now();
                  setPlayback(current => {
                    const previous = current[id];
                    const next = previous ? { ...previous,
                      elapsed: previous.startedAt === null ? previous.elapsed : now - previous.startedAt,
                      startedAt: previous.startedAt === null ? now - previous.elapsed : null,
                    } : { elapsed: 0, total: points.length, startedAt: now };
                    return { ...current, [id]: next };
                  });
                }}><i className={`bi bi-${playing ? 'pause-fill' : 'play-fill'}`} aria-hidden="true" /></IconButton>
            </Flex>
          </Flex>
          <Flex className="chart-legend" gap="3" wrap="wrap" mt="2" mb="1">
            <button type="button" aria-pressed={visible.player} onClick={() => setVisible(v => ({ ...v, player: !v.player }))} style={{ color: playerColor }}>━ {t('recoil.you')}</button>
            {reference.length > 0 && <button type="button" aria-pressed={visible.reference} onClick={() => setVisible(v => ({ ...v, reference: !v.reference }))} style={{ color: colors.accent }}>━ {t('recoil.reference')}</button>}
          </Flex>
          {points.length === 0 && reference.length === 0 ? <Flex align="center" justify="center" style={{ minHeight: 180 }}><Text size="2" color="gray">{t('recoil.empty')}</Text></Flex> : <>
            <Box className="recoil-plot"
              onPointerDownCapture={event => {
                if (event.button !== 0) return;
                event.currentTarget.setPointerCapture(event.pointerId);
                drag.current = { id, x: event.clientX, y: event.clientY };
                event.currentTarget.classList.add('dragging');
              }}
              onPointerMoveCapture={event => {
                if (drag.current?.id !== id) return;
                const rect = event.currentTarget.getBoundingClientRect();
                const dx = (event.clientX - drag.current.x) * 2 * halfSpan / Math.max(1, rect.width - inset - 20);
                const dy = (event.clientY - drag.current.y) * 2 * halfSpan / Math.max(1, rect.height - inset - 20);
                drag.current = { id, x: event.clientX, y: event.clientY };
                setPan(current => ({ x: current.x - dx, y: current.y + dy }));
              }}
              onLostPointerCapture={event => { drag.current = undefined; event.currentTarget.classList.remove('dragging'); }}
              role="img" aria-label={`${displayPlayerName(player?.name)} · ${label} · ${t('recoil.title')}`} style={{ aspectRatio: '1', width: '100%', maxWidth: 420, margin: 'auto' }}>
              <EChart height="100%" option={{
                tooltip: { trigger: 'item', renderMode: 'richText', formatter: (param: { dataIndex: number; seriesIndex: number }) => `${param.seriesIndex === 0 ? t('recoil.reference') : t('recoil.you')}\n${pointDescription((param.seriesIndex === 0 ? reference : points)[param.dataIndex]!, param.dataIndex)}` },
                grid: { left: inset, right: 20, top: 20, bottom: inset },
                xAxis: { type: 'value', min: cx - halfSpan, max: cx + halfSpan, name: t('recoil.horizontal'), nameLocation: 'middle', nameGap: typography[3]! + 16, axisLabel: { color: colors.muted, formatter: (value: number) => String(Number(value.toFixed(1))) }, splitLine: { lineStyle: { color: colors.border } } },
                yAxis: { type: 'value', min: cy - halfSpan, max: cy + halfSpan, axisLabel: { color: colors.muted, formatter: (value: number) => String(Number(value.toFixed(1))) }, splitLine: { lineStyle: { color: colors.border } } },
                series: [reference, points.slice(0, shown)].map((path, index) => ({ type: 'line', z: index + 2, data: (index === 0 ? visible.reference : visible.player) ? path.map(p => [p.x, p.y]) : [], symbol: 'circle', symbolSize: (_value: unknown, params: { dataIndex: number }) => index === 0 ? 5 : pulse && params.dataIndex === shown - 1 ? 10 : 4, showSymbol: true,
                  lineStyle: { width: index === 0 ? 2 : 1.5, color: index === 0 ? colors.accent : playerColor }, itemStyle: { color: index === 0 ? colors.accent : playerColor },
                  label: { show: false },
                })),
              }} />
            </Box>
            <Text as="p" size="1" color="gray" mb="2">{t('recoil.vertical')}</Text>
            {reference.length === 0 && <Text as="p" size="1" color="gray">{t('recoil.noReference')}</Text>}
            {points.length === 0 && <Text as="p" size="1" color="gray">{t('recoil.empty')}</Text>}
          </>}
        </Card>;
      })}
    </Grid>
    <Text as="p" size="1" color="gray" mt="3">{t('recoil.definition')}</Text>
  </Card>;
}
