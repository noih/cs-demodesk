import { displayPlayerName } from '../playerName.ts';
import { useEffect, useRef, useState } from 'react';
import { Box, Button, Card, Flex, Grid, Heading, IconButton, Select, Text } from '@radix-ui/themes';
import { useTranslation } from 'react-i18next';
import { useAppTheme } from '../AppTheme.tsx';
import { EChart } from '../charts/EChart.tsx';
import type { ParsedDemo, RecoilPoint } from '../api.ts';

import { averagePaths, projectReference, projectShots, stepRecoilZoom } from '../recoil.ts';

import calibration from '../data/recoil-reference.json';

const SHOT_INTERVAL_MS = 100;
const DEFAULT_ZOOM_SCALE = 0.84375;
const WEAPONS = [['ak47', 'AK-47'], ['m4a1', 'M4A4'], ['m4a1_silencer', 'M4A1-S']] as const;

export function RecoilChart({ parsed }: { parsed: ParsedDemo }) {
  const { t } = useTranslation();
  const { colors, typography } = useAppTheme();
  const [selected, setSelected] = useState(parsed.stats.find(p => Object.keys(p.recoil ?? {}).length)?.steamid ?? parsed.stats[0]?.steamid ?? '');
  const [burstSelection, setBurstSelection] = useState<Record<string, string>>({});
  const [visible, setVisible] = useState({ player: true, reference: true });
  const [playback, setPlayback] = useState<Record<string, { elapsed: number; total: number; startedAt: number | null }>>({});
  const isPlaying = Object.values(playback).some(state => state.startedAt !== null);
  useEffect(() => { setPlayback({}); setBurstSelection({}); }, [parsed, selected]);
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
      if (event.deltaY) setZoom(value => stepRecoilZoom(value, event.deltaY < 0 ? 1 : -1));
    };
    element.addEventListener('wheel', wheel, { passive: false });
    return () => element.removeEventListener('wheel', wheel);
  }, []);
  const player = parsed.stats.find(p => p.steamid === selected) ?? parsed.stats[0];
  const plots = WEAPONS.map(([id, label]) => {
    const bursts = player?.recoil?.[id] ?? [];
    const selection = burstSelection[id] ?? 'average';
    const burst = bursts.find(b => String(b.startTick) === selection);
    const points = burst ? projectShots(burst.shots) : averagePaths(bursts.map(b => projectShots(b.shots)));
    const standard = projectReference(calibration.weapons[id]).filter(p => p !== null);
    const reference = standard;
    const minX = Math.min(0, ...standard.map(p => p.x)), maxX = Math.max(0, ...standard.map(p => p.x));
    const minY = Math.min(0, ...standard.map(p => p.y)), maxY = Math.max(0, ...standard.map(p => p.y));
    return { id, label, points, reference, standard, bursts, selection: burst ? selection : 'average', cx: (minX + maxX) / 2, cy: (minY + maxY) / 2 };
  });
  const all = plots.flatMap(p => [...p.points, ...p.reference]).filter(p => p !== null);
  // Reference-only bounds keep each zoom percentage stable across players and bursts.
  const halfSpan = Math.max(10, ...plots.flatMap(p => p.standard
    .map(v => Math.max(Math.abs(v.x - p.cx), Math.abs(v.y - p.cy)) * 1.2))) / (zoom * 0.8 * DEFAULT_ZOOM_SCALE);
  const inset = Math.ceil(typography[3]! * 3 + 8);
  const playerColor = colors.players.split(',')[7]!;
  const pointDescription = (point: RecoilPoint | null, index: number) => point ? t('recoil.point', { shot: index + 1, x: point.x.toFixed(2), y: point.y.toFixed(2), n: point.samples }) : t('recoil.projectionLimit');

  return <Card ref={root} data-testid="recoil-chart">
    <Flex justify="between" align="center" gap="3" wrap="wrap" mb="2">
      <Heading data-text-role="subtitle" size="3">{t('recoil.title')}</Heading>
      <Flex className="recoil-controls" align="center" gap="3" wrap="wrap">
        <Flex align="center" gap="2">
          <IconButton size="1" variant="outline" disabled={zoom === 0.05 || !all.length} aria-label={t('replay.zoomOut')} title={t('replay.zoomOut')} onClick={() => setZoom(value => stepRecoilZoom(value, -1))}>−</IconButton>
          <Button size="1" variant="outline" disabled={!all.length} aria-label={t('recoil.resetZoom')} title={t('recoil.resetZoom')} onClick={() => { setZoom(1); setPan({ x: 0, y: 0 }); }} style={{ minWidth: 60, fontVariantNumeric: 'tabular-nums' }}>{Math.round(zoom * 100)}%</Button>
          <IconButton size="1" variant="outline" disabled={zoom === 2.5 || !all.length} aria-label={t('replay.zoomIn')} title={t('replay.zoomIn')} onClick={() => setZoom(value => stepRecoilZoom(value, 1))}>＋</IconButton>
        </Flex>
        {player && <Select.Root value={player.steamid} onValueChange={setSelected}>
          <Select.Trigger className="bounded-select" aria-label={t('recoil.player')} title={displayPlayerName(player.name)} />
          <Select.Content>{parsed.stats.map(p => <Select.Item key={p.steamid} value={p.steamid}>{displayPlayerName(p.name)}</Select.Item>)}</Select.Content>
        </Select.Root>}
      </Flex>
    </Flex>
    <Grid style={{ gridTemplateColumns: 'repeat(auto-fit, minmax(min(100%, 360px), 1fr))' }} gap="3">
      {plots.map(({ id, label, points, reference, bursts, selection, cx: referenceX, cy: referenceY }) => {
        const cx = referenceX + pan.x, cy = referenceY + pan.y;
        const state = playback[id];
        const playing = !!state && state.startedAt !== null;
        const shown = state ? Math.min(points.length, Math.floor(state.elapsed / SHOT_INTERVAL_MS) + 1) : points.length;
        const pulse = playing && state.elapsed % SHOT_INTERVAL_MS < SHOT_INTERVAL_MS / 2;
        const playbackLabel = `${t(playing ? 'replay.pause' : 'replay.play')} ${label}`;
        const choices = ['average', ...bursts.map(b => String(b.startTick))];
        const choiceIndex = choices.indexOf(selection);
        const selectBurst = (value: string) => {
          setBurstSelection(current => ({ ...current, [id]: value }));
          setPlayback(current => { const next = { ...current }; delete next[id]; return next; });
        };
        return <Card key={id} style={{ minWidth: 0 }}>
          <Flex justify="between" align="center" gap="2">
            <Heading data-text-role="body-heading" size="2">{label}</Heading>
            <Text size="1" color="gray">{t('recoil.bursts', { n: bursts.length })}</Text>
          </Flex>
          <Flex className="recoil-controls" gap="1" mt="4" align="center" style={{ width: '100%' }}>
            <IconButton variant="outline" disabled={choiceIndex <= 0} aria-label={`${label} ${t('recoil.previousBurst')}`} onClick={() => selectBurst(choices[choiceIndex - 1]!)} style={{ flexShrink: 0 }}>〈</IconButton>
            <Select.Root value={selection} onValueChange={selectBurst}>
              <Select.Trigger aria-label={`${label} ${t('recoil.view')}`} style={{ flex: 1, minWidth: 0 }} />
              <Select.Content>
                <Select.Item value="average">{t('recoil.average')}</Select.Item>
                {bursts.map((b, i) => <Select.Item key={b.startTick} value={String(b.startTick)}>{t('recoil.burst', { n: i + 1, round: b.round, tick: b.startTick, shots: b.shots.length })}</Select.Item>)}
              </Select.Content>
            </Select.Root>
            <IconButton variant="outline" disabled={choiceIndex >= choices.length - 1} aria-label={`${label} ${t('recoil.nextBurst')}`} onClick={() => selectBurst(choices[choiceIndex + 1]!)} style={{ flexShrink: 0 }}>〉</IconButton>
          </Flex>
          <Flex className="chart-legend" gap="3" wrap="wrap" mt="2" mb="1">
            <button type="button" aria-pressed={visible.player} onClick={() => setVisible(v => ({ ...v, player: !v.player }))} style={{ color: playerColor }}>━ {t('recoil.you')}</button>
            {reference.length > 0 && <button type="button" aria-pressed={visible.reference} onClick={() => setVisible(v => ({ ...v, reference: !v.reference }))} style={{ color: colors.accent }}>━ {t('recoil.reference')}</button>}
          </Flex>
          {points.length === 0 && reference.length === 0 ? <Flex align="center" justify="center" style={{ minHeight: 180 }}><Text size="2" color="gray">{t('recoil.empty')}</Text></Flex> : <>
            <Box style={{ position: 'relative', aspectRatio: '1', width: '100%', maxWidth: 420, margin: 'auto' }}>
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
              role="img" aria-label={`${displayPlayerName(player?.name)} · ${label} · ${t('recoil.title')}`} style={{ width: '100%', height: '100%' }}>
              <EChart height="100%" option={{
                tooltip: { trigger: 'item', renderMode: 'richText', formatter: (param: { dataIndex: number; seriesIndex: number }) => `${param.seriesIndex === 0 ? t('recoil.reference') : t('recoil.you')}\n${pointDescription((param.seriesIndex === 0 ? reference : points)[param.dataIndex]!, param.dataIndex)}` },
                grid: { left: inset, right: 20, top: 20, bottom: inset, outerBoundsMode: 'none' },
                xAxis: { type: 'value', min: cx - halfSpan, max: cx + halfSpan, name: t('recoil.horizontal'), nameLocation: 'middle', nameGap: typography[3]! + 16, axisLabel: { color: colors.muted, formatter: (value: number) => String(Number(value.toFixed(1))) }, splitLine: { lineStyle: { color: colors.border } } },
                yAxis: { type: 'value', min: cy - halfSpan, max: cy + halfSpan, axisLabel: { color: colors.muted, formatter: (value: number) => String(Number(value.toFixed(1))) }, splitLine: { lineStyle: { color: colors.border } } },
                series: [reference, points.slice(0, shown)].map((path, index) => ({ type: 'line', z: index + 2, data: (index === 0 ? visible.reference : visible.player) ? path.map(p => p ? [p.x, p.y] : null) : [], symbol: 'circle', symbolSize: (_value: unknown, params: { dataIndex: number }) => index === 0 ? 5 : pulse && params.dataIndex === shown - 1 ? 10 : 4, showSymbol: true,
                  lineStyle: { width: index === 0 ? 2 : 1.5, color: index === 0 ? colors.accent : playerColor }, itemStyle: { color: index === 0 ? colors.accent : playerColor },
                  label: { show: false },
                  markLine: index === 1 ? { silent: true, symbol: 'none', label: { show: false }, lineStyle: { color: colors.muted, type: 'dashed' }, data: [{ xAxis: 0 }, { yAxis: 0 }] } : undefined,
                })),
              }} />
            </Box>
            <IconButton size="2" variant="ghost" style={{ position: 'absolute', right: 20 + 8, bottom: inset + 8, margin: 0, padding: 0, boxSizing: 'border-box', width: 30, height: 30, zIndex: 1 }} disabled={!points.length} aria-label={playbackLabel} title={playbackLabel}
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
              }}><svg width="15" height="15" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true" style={{ display: 'block' }}><path d={playing ? 'M4 3h3v10H4zm5 0h3v10H9z' : 'M5 3v10l8-5z'} /></svg></IconButton>
            </Box>
            {[...points, ...reference].some(p => p === null) && <Text as="p" size="1" color="gray">{t('recoil.projectionLimit')}</Text>}
            {reference.length === 0 && <Text as="p" size="1" color="gray">{t('recoil.noReference')}</Text>}
            {points.length === 0 && <Text as="p" size="1" color="gray">{t('recoil.empty')}</Text>}
          </>}

        </Card>;
      })}
    </Grid>
    <Text as="p" size="1" color="gray" mt="2">{t('recoil.vertical')}</Text>
    <Text as="p" size="1" color="gray" mt="2">{t('recoil.definition')}</Text>
  </Card>;
}
