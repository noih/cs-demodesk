import { useMemo, useState } from 'react';
import { Badge, Box, Button, Callout, Checkbox, DataList, Dialog, Flex, Grid, SegmentedControl, Select, Slider, Switch, Table, Text } from '@radix-ui/themes';
import { VideoIcon } from '@radix-ui/react-icons';
import { useTranslation } from 'react-i18next';
import { api, clock, errorText, DEFAULT_RENDER_OPTIONS, type DemoMeta, type Highlight, type ParsedDemo, type RenderOptions, type Status } from '../api.ts';

const RESOLUTIONS = [
  { label: '720p', width: 1280, height: 720 },
  { label: '1080p', width: 1920, height: 1080 },
  { label: '1440p', width: 2560, height: 1440 },
];
const HOT_TAGS = new Set(['ace', '4k', 'clutch', 'knife', 'noscope']);
/** HUD switches in the export dialog, in display order. */
const HUD_TOGGLES = ['hud', 'crosshair', 'radar', 'killFeed', 'chat', 'viewmodel', 'tracers', 'xray', 'trueView'] as const;

function summaryOf(h: Highlight): string {
  return h.title.replace(`${h.player.name} — `, '').replace(/ · R\d+$/, '');
}

export function HighlightsTab({ meta, parsed, status, onRendered }: { meta: DemoMeta; parsed: ParsedDemo; status?: Status; onRendered: () => void }) {
  const { t } = useTranslation();
  const [playerFilter, setPlayerFilter] = useState<string>('all');
  const [minScore, setMinScore] = useState(3);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [opts, setOpts] = useState<RenderOptions>(DEFAULT_RENDER_OPTIONS);
  const [submitting, setSubmitting] = useState(false);
  const [dialog, setDialog] = useState(false);
  const tr = parsed.info.tickRate;

  const visible = useMemo(() => parsed.highlights.filter((h) => (playerFilter === 'all' || h.player.steamid === playerFilter) && h.score >= minScore), [parsed.highlights, playerFilter, minScore]);
  const toggle = (id: string) =>
    setSelected((s) => {
      const n = new Set(s);
      n.has(id) ? n.delete(id) : n.add(id);
      return n;
    });
  const allVisibleSelected = visible.length > 0 && visible.every((h) => selected.has(h.id));
  const chosen = parsed.highlights.filter((h) => selected.has(h.id));
  const selectedSeconds = chosen.reduce((s, h) => s + (h.endTick - h.startTick) / tr, 0);

  const render = async () => {
    if (submitting) return;
    setSubmitting(true);
    try {
      await api.render(meta.id, [...selected], opts);
      setDialog(false);
      setSelected(new Set());
      onRendered();
    } catch (e) {
      alert(errorText(e));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Box>
      <Flex gap="4" align="center" wrap="wrap" mb="3">
        <Select.Root value={playerFilter} onValueChange={setPlayerFilter}>
          <Select.Trigger style={{ minWidth: 200 }} />
          <Select.Content>
            <Select.Item value="all">{t('highlights.allPlayers')}</Select.Item>
            {parsed.stats.map((p) => (
              <Select.Item key={p.steamid} value={p.steamid}>
                {p.name} ({p.kills}K/{p.deaths}D)
              </Select.Item>
            ))}
          </Select.Content>
        </Select.Root>
        <Flex align="center" gap="3" style={{ width: 260 }}>
          <Text size="2" style={{ whiteSpace: 'nowrap' }}>
            {t('highlights.minScore', { n: minScore })}
          </Text>
          <Slider min={0} max={15} step={1} value={[minScore]} onValueChange={([v]) => setMinScore(v ?? 0)} style={{ flex: 1 }} />
        </Flex>
        <Box style={{ flex: 1 }} />
        <Text size="2" color="gray">
          {t('highlights.shown', { shown: visible.length, total: parsed.highlights.length })}
        </Text>
        <Button variant="soft" size="2" onClick={() => setSelected(new Set(allVisibleSelected ? [] : visible.map((h) => h.id)))}>
          {allVisibleSelected ? t('highlights.deselectAll') : t('highlights.selectAll')}
        </Button>
        <Button size="2" disabled={selected.size === 0 || submitting} onClick={() => setDialog(true)}>
          <VideoIcon /> {selected.size ? t('highlights.exportN', { n: selected.size }) : t('highlights.export')}
        </Button>
      </Flex>

      <Table.Root className="nowrap-headers" variant="surface" size="1" layout="auto">
        <Table.Header>
          <Table.Row>
            <Table.ColumnHeaderCell width="36px" />
            <Table.ColumnHeaderCell align="right">{t('highlights.col.score')}</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right">{t('highlights.col.round')}</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell>{t('highlights.col.time')}</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell align="right">{t('highlights.col.length')}</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell>{t('highlights.col.player')}</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell>{t('highlights.col.what')}</Table.ColumnHeaderCell>
            <Table.ColumnHeaderCell>{t('highlights.col.tags')}</Table.ColumnHeaderCell>
          </Table.Row>
        </Table.Header>
        <Table.Body>
          {visible.map((h) => (
            <Table.Row key={h.id} className={`row-click ${selected.has(h.id) ? 'row-selected' : ''}`} onClick={() => toggle(h.id)}>
              <Table.Cell>
                <Checkbox checked={selected.has(h.id)} onCheckedChange={() => toggle(h.id)} onClick={(e) => e.stopPropagation()} />
              </Table.Cell>
              <Table.Cell align="right">
                <Text weight="bold">{h.score.toFixed(1)}</Text>
              </Table.Cell>
              <Table.Cell align="right">{h.round}</Table.Cell>
              <Table.Cell>{clock(h.startTick, tr)}</Table.Cell>
              <Table.Cell align="right">{Math.round((h.endTick - h.startTick) / tr)}s</Table.Cell>
              <Table.Cell>
                <Text truncate style={{ maxWidth: 160, display: 'block' }}>
                  {h.player.name}
                </Text>
              </Table.Cell>
              <Table.Cell style={{ whiteSpace: 'nowrap' }}>{summaryOf(h)}</Table.Cell>
              <Table.Cell>
                <Flex gap="1" wrap="wrap">
                  {h.tags.map((tag) => (
                    <Badge key={tag} size="1" color={HOT_TAGS.has(tag) ? 'amber' : 'gray'} variant={HOT_TAGS.has(tag) ? 'solid' : 'soft'}>
                      {tag}
                    </Badge>
                  ))}
                </Flex>
              </Table.Cell>
            </Table.Row>
          ))}
          {visible.length === 0 && (
            <Table.Row>
              <Table.Cell colSpan={8}>
                <Text color="gray">{t('highlights.empty')}</Text>
              </Table.Cell>
            </Table.Row>
          )}
        </Table.Body>
      </Table.Root>

      <Dialog.Root open={dialog} onOpenChange={setDialog}>
        <Dialog.Content maxWidth="1000px">
          <Dialog.Title>{t('highlights.dialogTitle')}</Dialog.Title>
          <Dialog.Description size="2" color="gray">
            {t('highlights.summary', { n: chosen.length, seconds: Math.round(selectedSeconds) })}
          </Dialog.Description>
          <Grid columns={{ initial: '1', sm: '260px 260px minmax(0, 1fr)' }} gap="5" mt="4" align="start">
            <Flex direction="column" gap="3">
              {chosen.length > 1 && (
                <Text as="label" size="2">
                  <Flex gap="2" align="center">
                    <Switch size="1" checked={opts.merge} onCheckedChange={(v) => setOpts({ ...opts, merge: v })} />
                    {t('highlights.merge')}
                  </Flex>
                </Text>
              )}
              <Box>
                <Text as="div" size="1" color="gray" mb="1">
                  {t('highlights.sizeLimit')}
                  {chosen.length > 1 && (opts.merge ? t('highlights.sizeMerged') : t('highlights.sizeEach'))}
                </Text>
                <SegmentedControl.Root size="1" value={String(opts.maxSizeMb)} onValueChange={(v) => setOpts({ ...opts, maxSizeMb: v === 'null' ? null : Number(v) })} style={{ width: '100%' }}>
                  <SegmentedControl.Item value="10">10 MB</SegmentedControl.Item>
                  <SegmentedControl.Item value="20">20 MB</SegmentedControl.Item>
                  <SegmentedControl.Item value="50">50 MB</SegmentedControl.Item>
                  <SegmentedControl.Item value="null">{t('highlights.unlimited')}</SegmentedControl.Item>
                </SegmentedControl.Root>
              </Box>
              <Box>
                <Text as="div" size="1" color="gray" mb="1">
                  {t('common.resolution')}
                </Text>
                <SegmentedControl.Root
                  size="1"
                  value={String(opts.width)}
                  onValueChange={(v) => {
                    const r = RESOLUTIONS.find((x) => x.width === Number(v))!;
                    setOpts({ ...opts, width: r.width, height: r.height });
                  }}
                  style={{ width: '100%' }}
                >
                  {RESOLUTIONS.map((r) => (
                    <SegmentedControl.Item key={r.width} value={String(r.width)}>
                      {r.label}
                    </SegmentedControl.Item>
                  ))}
                </SegmentedControl.Root>
              </Box>
              <Box>
                <Text as="div" size="1" color="gray" mb="1">
                  FPS
                </Text>
                <SegmentedControl.Root size="1" value={String(opts.fps)} onValueChange={(v) => setOpts({ ...opts, fps: Number(v) })} style={{ width: '100%' }}>
                  <SegmentedControl.Item value="30">30</SegmentedControl.Item>
                  <SegmentedControl.Item value="60">60</SegmentedControl.Item>
                </SegmentedControl.Root>
              </Box>
              <Box>
                <Text as="div" size="1" color="gray" mb="1">
                  {t('highlights.encoder')}
                </Text>
                <Select.Root value={opts.codec} onValueChange={(v) => setOpts({ ...opts, codec: v })} size="1">
                  <Select.Trigger style={{ width: '100%' }} />
                  <Select.Content>
                    <Select.Item value="libx264">H.264 · CPU（libx264）</Select.Item>
                    <Select.Item value="libx265">H.265 · CPU（libx265）</Select.Item>
                    <Select.Item value="h264_nvenc">H.264 · NVIDIA（h264_nvenc）</Select.Item>
                    <Select.Item value="hevc_nvenc">H.265 · NVIDIA（hevc_nvenc）</Select.Item>
                  </Select.Content>
                </Select.Root>
              </Box>
              <Text as="label" size="1">
                <Flex align="center" justify="between" gap="3">
                  {t('highlights.showGame')}
                  <Switch size="1" checked={opts.showGame} onCheckedChange={(showGame) => setOpts({ ...opts, showGame })} />
                </Flex>
              </Text>
            </Flex>
            <Flex direction="column" gap="3">
              <DataList.Root size="1">
                {HUD_TOGGLES.map((key) => (
                  <DataList.Item key={key} align="center">
                    <DataList.Label>{t(`highlights.toggle.${key}`)}</DataList.Label>
                    <DataList.Value>
                      <Switch size="1" checked={opts[key]} disabled={key === 'crosshair' && !opts.hud} onCheckedChange={(v) => setOpts({ ...opts, [key]: v })} />
                    </DataList.Value>
                  </DataList.Item>
                ))}
              </DataList.Root>
              <Box>
                <Text as="div" size="1" color="gray" mb="1">
                  {t('highlights.hudScale', { n: opts.hudScale.toFixed(2) })}
                </Text>
                <Slider min={0.5} max={0.95} step={0.05} value={[opts.hudScale]} onValueChange={([v]) => setOpts({ ...opts, hudScale: v ?? 0.85 })} />
              </Box>
            </Flex>
            <Box>
              <Text as="div" size="1" color="gray" mb="1">
                {t('highlights.clips')}
              </Text>
              <Box style={{ maxHeight: 340, overflow: 'auto', border: '1px solid var(--gray-a5)', borderRadius: 'var(--radius-2)' }}>
                <Table.Root className="nowrap-headers" size="1">
                  <Table.Header>
                    <Table.Row>
                      <Table.ColumnHeaderCell align="right" width="48px">
                        {t('highlights.col.round')}
                      </Table.ColumnHeaderCell>
                      <Table.ColumnHeaderCell>{t('highlights.col.player')}</Table.ColumnHeaderCell>
                      <Table.ColumnHeaderCell>{t('highlights.col.what')}</Table.ColumnHeaderCell>
                      <Table.ColumnHeaderCell align="right" width="48px">
                        {t('highlights.col.length')}
                      </Table.ColumnHeaderCell>
                    </Table.Row>
                  </Table.Header>
                  <Table.Body>
                    {[...chosen]
                      .sort((a, b) => a.startTick - b.startTick)
                      .map((h) => (
                        <Table.Row key={h.id}>
                          <Table.Cell align="right">{h.round}</Table.Cell>
                          <Table.Cell>
                            <Text truncate style={{ display: 'block', maxWidth: 100 }}>
                              {h.player.name}
                            </Text>
                          </Table.Cell>
                          <Table.Cell style={{ whiteSpace: 'nowrap' }}>{summaryOf(h)}</Table.Cell>
                          <Table.Cell align="right">{Math.round((h.endTick - h.startTick) / tr)}s</Table.Cell>
                        </Table.Row>
                      ))}
                  </Table.Body>
                </Table.Root>
              </Box>
            </Box>
          </Grid>
          {status && !status.ok && (
            <Callout.Root color="red" size="1" mt="4">
              <Callout.Text>{t('highlights.notReady')}</Callout.Text>
            </Callout.Root>
          )}
          <Flex justify="between" align="center" mt="4" gap="3">
            <Text size="1" color="gray">
              {t(opts.showGame ? 'highlights.visibleGame' : 'highlights.hiddenGame')}
            </Text>
            <Flex gap="3">
              <Dialog.Close>
                <Button variant="soft" color="gray">
                  {t('common.cancel')}
                </Button>
              </Dialog.Close>
              <Button disabled={submitting || !status?.ok} onClick={() => void render()}>
                {submitting ? t('highlights.submitting') : t('highlights.export')}
              </Button>
            </Flex>
          </Flex>
        </Dialog.Content>
      </Dialog.Root>
    </Box>
  );
}
