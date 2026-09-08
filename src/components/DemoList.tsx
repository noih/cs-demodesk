import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Badge, Box, Callout, Flex, IconButton, Spinner, Text, TextField, Tooltip } from '@radix-ui/themes';
import { ClockIcon, Cross2Icon, ExclamationTriangleIcon, GearIcon, MagnifyingGlassIcon, PlusIcon, ReloadIcon, VideoIcon } from '@radix-ui/react-icons';
import { useVirtualizer } from '@tanstack/react-virtual';
import { open } from '@tauri-apps/plugin-dialog';
import { api, errorText, mb, type DemoMeta, type RenderJob } from '../api.ts';
import { fmtDate, fmtTime } from '../i18n/index.ts';
import { DateField, dayOf } from './DateField.tsx';

const STATUS_COLOR: Record<DemoMeta['status'], 'gray' | 'amber' | 'green' | 'red'> = { new: 'gray', parsing: 'amber', parsed: 'green', error: 'red' };
const ROW_HEIGHT = 100;

/** "match730_003841245499151614385_1512260798_142.dem" → "match730_…_142" */
function shortName(name: string): string {
  const base = name.replace(/\.dem$/i, '');
  if (base.length <= 28) return base;
  const parts = base.split('_');
  return parts.length >= 3 ? `${parts[0]}_…_${parts[parts.length - 1]}` : `${base.slice(0, 14)}…${base.slice(-10)}`;
}

export function DemoList({
  demos,
  jobs,
  selectedId,
  settingsOpen,
  onSelect,
  onToggleSettings,
  onChanged,
  onRefresh,
}: {
  demos: DemoMeta[];
  jobs: RenderJob[];
  selectedId?: string;
  settingsOpen: boolean;
  onSelect: (id: string) => void;
  onToggleSettings: () => void;
  onChanged: () => Promise<void>;
  onRefresh: () => Promise<boolean>;
}) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const [refreshStatus, setRefreshStatus] = useState<'refreshing' | 'refreshed' | 'refreshFailed'>();
  const refreshRequest = useRef(0);
  useEffect(() => () => { refreshRequest.current++; }, []);
  useEffect(() => {
    if (!refreshStatus || refreshStatus === 'refreshing') return;
    const timer = setTimeout(() => setRefreshStatus(undefined), 3000);
    return () => clearTimeout(timer);
  }, [refreshStatus]);
  const handleRefresh = async () => {
    const request = ++refreshRequest.current;
    setRefreshStatus('refreshing');
    try {
      const success = await onRefresh();
      if (request === refreshRequest.current) setRefreshStatus(success ? 'refreshed' : 'refreshFailed');
    } catch {
      if (request === refreshRequest.current) setRefreshStatus('refreshFailed');
    }
  };
  const [query, setQuery] = useState('');
  const [from, setFrom] = useState('');
  const [to, setTo] = useState('');
  const scrollRef = useRef<HTMLDivElement>(null);

  const exportsByDemo = useMemo(() => {
    const summaries = new Map<string, { videos: number; queued: number; running: number; error: number }>();
    for (const job of jobs) {
      const summary = summaries.get(job.demoId) ?? { videos: 0, queued: 0, running: 0, error: 0 };
      summary.videos += job.outputs.length;
      if (job.status === 'queued' || job.status === 'running' || job.status === 'error') summary[job.status]++;
      summaries.set(job.demoId, summary);
    }
    return summaries;
  }, [jobs]);

  const visible = useMemo(() => {
    // Substring match, case-insensitive; several words must all appear (any order),
    // across file name, map name and player names.
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    return demos.filter((d) => {
      if (words.length) {
        const hay = [d.name, d.mapName ?? '', ...(d.summary?.players ?? [])].join(' ').toLowerCase();
        if (!words.every((w) => hay.includes(w))) return false;
      }
      const day = dayOf(new Date(d.mtimeMs));
      if (from && day < from) return false;
      if (to && day > to) return false;
      return true;
    });
  }, [demos, query, from, to]);

  const virtualizer = useVirtualizer({
    count: visible.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 6,
  });

  /** Pick .dem files; the backend copies them into the replays folder and we parse them right away. */
  const addFiles = async () => {
    const picked = await open({ multiple: true, filters: [{ name: 'CS2 demo', extensions: ['dem'] }] });
    if (!picked) return;
    const paths = Array.isArray(picked) ? picked : [picked];
    setBusy(true);
    try {
      const metas = [];
      for (const p of paths) metas.push(await api.registerDemo(p));
      for (const m of metas) if (m.status === 'new') await api.parse(m.id);
      await onChanged();
      if (metas[0]) onSelect(metas[0].id);
    } catch (e) {
      alert(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  const filtered = visible.length !== demos.length;

  return (
    <Flex direction="column" style={{ flex: 1, minHeight: 0 }}>
      <div role="status" aria-live="polite" aria-atomic="true" style={{ position: 'fixed', right: 24, bottom: 24, zIndex: 100, pointerEvents: 'none' }}>
        {refreshStatus && (
          <Callout.Root size="1" color={refreshStatus === 'refreshFailed' ? 'red' : 'green'} variant="surface">
            <Callout.Text>{t(`demoList.${refreshStatus}`)}</Callout.Text>
          </Callout.Root>
        )}
      </div>
      <Flex direction="column" gap="2" px="3" py="3">
        <Flex align="center" gap="2">
          <Tooltip content={t('demoList.addTooltip')}>
            <IconButton size="2" onClick={() => void addFiles()} disabled={busy} aria-label={t('demoList.add')}>
              <PlusIcon />
            </IconButton>
          </Tooltip>
          <Tooltip content={t('demoList.rescanTooltip')}>
            <IconButton size="2" variant="soft" onClick={() => void handleRefresh()} disabled={busy} aria-label={t('demoList.rescan')}>
              {refreshStatus === 'refreshing' ? <Spinner size="1" /> : <ReloadIcon />}
            </IconButton>
          </Tooltip>
          <Text size="2" color="gray" style={{ flex: 1 }}>
            {filtered ? t('demoList.countFiltered', { shown: visible.length, total: demos.length }) : t('demoList.count', { count: demos.length })}
          </Text>
          <Tooltip content={t('common.settings')}>
            <IconButton size="2" variant={settingsOpen ? 'solid' : 'soft'} onClick={onToggleSettings} aria-label={t('common.settings')}>
              <GearIcon />
            </IconButton>
          </Tooltip>
        </Flex>
        <TextField.Root size="1" placeholder={t('demoList.searchPlaceholder')} value={query} onChange={(e) => setQuery(e.target.value)}>
          <TextField.Slot>
            <MagnifyingGlassIcon />
          </TextField.Slot>
          {query && (
            <TextField.Slot side="right">
              <IconButton size="1" variant="ghost" color="gray" onClick={() => setQuery('')} aria-label={t('demoList.clearSearch')}>
                <Cross2Icon />
              </IconButton>
            </TextField.Slot>
          )}
        </TextField.Root>
        <Flex align="center" gap="1">
          <DateField value={from} max={to || undefined} onChange={setFrom} label={t('demoList.from')} />
          <Text size="1" color="gray">
            –
          </Text>
          <DateField value={to} min={from || undefined} onChange={setTo} label={t('demoList.to')} />
          {(from || to) && (
            <IconButton
              size="1"
              variant="surface"
              color="gray"
              onClick={() => {
                setFrom('');
                setTo('');
              }}
              aria-label={t('demoList.clearDates')}
            >
              <Cross2Icon />
            </IconButton>
          )}
        </Flex>
      </Flex>

      <div ref={scrollRef} className="demo-scroll">
        {visible.length > 0 ? (
          <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
            {virtualizer.getVirtualItems().map((row) => {
              const d = visible[row.index]!;
              const exports = exportsByDemo.get(d.id);
              return (
                <Tooltip key={d.id} content={d.path} side="right">
                  <div className={`demo-item ${d.id === selectedId ? 'active' : ''}`} style={{ position: 'absolute', top: 0, left: 0, width: '100%', height: row.size, transform: `translateY(${row.start}px)` }} onClick={() => onSelect(d.id)}>
                    <Flex justify="between" align="center" gap="2">
                      <Text weight="bold" truncate>
                        {d.mapName ?? shortName(d.name)}
                      </Text>
                      <Badge size="1" color={STATUS_COLOR[d.status]} variant="soft" style={{ flex: 'none' }}>
                        {t(`demoList.status.${d.status}`)}
                      </Badge>
                    </Flex>
                    <Text as="div" size="1" color="gray" truncate>
                      {d.summary ? (
                        <>
                          <Text color="blue">{d.summary.scoreA}</Text> – <Text color="orange">{d.summary.scoreB}</Text> · {t('demoList.highlights', { count: d.summary.highlights })}
                        </>
                      ) : (
                        shortName(d.name)
                      )}
                    </Text>
                    <Text as="div" size="1" color="gray" truncate>
                      {fmtDate(d.mtimeMs)} {fmtTime(d.mtimeMs)} · {mb(d.bytes)}
                    </Text>
                    <Flex align="center" gap="2" mt="1" style={{ whiteSpace: 'nowrap' }}>
                      <Badge size="1" color={exports?.videos ? 'green' : 'gray'} variant="soft">
                        <VideoIcon /> {t('demoList.videos', { n: exports?.videos ?? 0 })}
                      </Badge>
                      {exports && exports.queued > 0 && (
                        <Badge size="1" color="amber" title={t('demoList.exportJobs', { status: t('renders.status.queued'), n: exports.queued })} aria-label={t('demoList.exportJobs', { status: t('renders.status.queued'), n: exports.queued })}>
                          <ClockIcon /> {exports.queued}
                        </Badge>
                      )}
                      {exports && exports.running > 0 && (
                        <Badge size="1" color="blue" title={t('demoList.exportJobs', { status: t('renders.status.running'), n: exports.running })} aria-label={t('demoList.exportJobs', { status: t('renders.status.running'), n: exports.running })}>
                          <ReloadIcon /> {exports.running}
                        </Badge>
                      )}
                      {exports && exports.error > 0 && (
                        <Badge size="1" color="red" title={t('demoList.exportJobs', { status: t('renders.status.error'), n: exports.error })} aria-label={t('demoList.exportJobs', { status: t('renders.status.error'), n: exports.error })}>
                          <ExclamationTriangleIcon /> {exports.error}
                        </Badge>
                      )}
                    </Flex>
                  </div>
                </Tooltip>
              );
            })}
          </div>
        ) : (
          <Box p="4">
            <Text size="2" color="gray">
              {demos.length === 0 ? t('demoList.empty') : t('demoList.noMatch')}
            </Text>
          </Box>
        )}
      </div>
    </Flex>
  );
}
