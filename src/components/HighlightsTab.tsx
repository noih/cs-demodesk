import { ExportDialog } from './ExportDialog.tsx';
import { displayPlayerName } from '../playerName.ts';
import { useMemo, useState } from 'react';
import { Badge, Box, Button, Checkbox, Flex, Select, Slider, Table, Text } from '@radix-ui/themes';
import { useTranslation } from 'react-i18next';
import { api, clock, type DemoMeta, type Highlight, type ParsedDemo, type Status } from '../api.ts';

const HOT_TAGS = new Set(['ace', '4k', 'clutch', 'knife', 'noscope']);

function summaryOf(h: Highlight): string {
  return h.title.replace(`${displayPlayerName(h.player.name)} — `, '').replace(/ · R\d+$/, '');
}

export function HighlightsTab({ meta, parsed, status, onRendered, onSetup }: { meta: DemoMeta; parsed: ParsedDemo; status?: Status; onRendered: () => void; onSetup: () => void }) {
  const { t } = useTranslation();
  const [playerFilter, setPlayerFilter] = useState<string>('all');
  const [minScore, setMinScore] = useState(3);
  const [selected, setSelected] = useState<Set<string>>(new Set());
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

  return (
    <Box>
      <Flex gap="4" align="center" wrap="wrap" mb="3">
        <Select.Root value={playerFilter} onValueChange={setPlayerFilter}>
          <Select.Trigger className="bounded-select" title={playerFilter === 'all' ? t('highlights.allPlayers') : displayPlayerName(parsed.stats.find(p => p.steamid === playerFilter)?.name)} />
          <Select.Content>
            <Select.Item value="all">{t('highlights.allPlayers')}</Select.Item>
            {parsed.stats.map((p) => (
              <Select.Item key={p.steamid} value={p.steamid}>
                {displayPlayerName(p.name)} ({p.kills}K/{p.deaths}D)
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
        <Button size="2" disabled={selected.size === 0} onClick={() => setDialog(true)}>
          <i aria-hidden="true" className="bi bi-camera-video app-icon"  /> {selected.size ? t('highlights.exportN', { n: selected.size }) : t('highlights.export')}
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
                  {displayPlayerName(h.player.name)}
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

      <ExportDialog open={dialog} onOpenChange={setDialog} count={chosen.length} seconds={selectedSeconds} status={status} onSetup={onSetup}
        onSubmit={options=>api.render(meta.id,[...selected],options)} onSubmitted={()=>{setSelected(new Set());onRendered();}} />
    </Box>
  );
}
