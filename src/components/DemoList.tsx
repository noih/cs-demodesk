import { useAppTheme } from '../AppTheme.tsx';
import { Spinner } from './Spinner.tsx';
import { useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { format } from 'date-fns';
import { useTranslation } from 'react-i18next';
import { Badge, Box, Callout, Flex, IconButton, Popover, Text, TextField, Tooltip } from '@radix-ui/themes';
import { useVirtualizer } from '@tanstack/react-virtual';
import { open } from '@tauri-apps/plugin-dialog';
import { api, errorText, type DemoMeta, type RenderJob } from '../api.ts';
import { DateField, dayOf } from './DateField.tsx';

const STATUS_COLOR: Record<DemoMeta['status'], 'gray' | 'amber' | 'green' | 'red'> = { new: 'gray', parsing: 'amber', parsed: 'green', error: 'red' };


export function DemoList({
  demos,
  jobs,
  selectedId,
  toolbar,
  selectionRequest,
  onSelect,

  onChanged,
  onRefresh,
}: {
  demos: DemoMeta[];
  jobs: RenderJob[];
  selectedId?: string;
  toolbar: HTMLElement | null;
  selectionRequest: number;
  onSelect: (id: string) => void;

  onChanged: () => Promise<void>;
  onRefresh: () => Promise<boolean>;
}) {
  const { t } = useTranslation();
  const { typography } = useAppTheme();
  const rowHeight = 40 + typography[2]! * 3 + typography[3]! * 2;
  const [busy, setBusy] = useState(false);
  const [refreshStatus, setRefreshStatus] = useState<'refreshing' | 'refreshed' | 'refreshFailed'>();
  const refreshRequest = useRef(0);
  useEffect(() => () => { refreshRequest.current++; }, []);
  useEffect(() => {
    if (!refreshStatus || refreshStatus === 'refreshing' || refreshStatus === 'refreshFailed') return;
    const timer = setTimeout(() => setRefreshStatus(undefined), 3000);
    return () => clearTimeout(timer);
  }, [refreshStatus]);
  const handleRefresh = async () => {
    if (refreshStatus === 'refreshing') return;
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
    estimateSize: () => rowHeight,
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
  const handledSelection = useRef(0);
  const filtered = !!(query.trim() || from || to);
  useEffect(() => {
    if (!selectionRequest) return;
    setQuery(''); setFrom(''); setTo('');
  }, [selectionRequest]);
  useEffect(() => {
    if (!selectionRequest || handledSelection.current === selectionRequest || query || from || to) return;
    const index = visible.findIndex(d => d.id === selectedId);
    if (index >= 0) {
      virtualizer.scrollToIndex(index, { align: 'auto' });
      handledSelection.current = selectionRequest;
    }
  }, [selectionRequest, selectedId, visible, query, from, to, virtualizer]);

  useEffect(() => { virtualizer.measure(); }, [rowHeight, virtualizer]);

  return (
    <Flex direction="column" style={{ flex: 1, minHeight: 0 }}>
      <div role="status" aria-live="polite" aria-atomic="true" className="refresh-notice">
        {refreshStatus && (
          <Callout.Root size="1" color={refreshStatus === 'refreshFailed' ? 'red' : 'green'} variant="surface">
            <Callout.Text>{t(`demoList.${refreshStatus}`)}</Callout.Text>
            <IconButton size="2" variant="ghost" aria-label={t('common.close')} onClick={() => setRefreshStatus(undefined)}><i aria-hidden="true" className="bi bi-x-lg app-icon"  /></IconButton>
          </Callout.Root>
        )}
      </div>
      {toolbar && createPortal(<Flex align="center" gap="2">
        <Flex align="center" gap="2">
          <Tooltip delayDuration={150} content={t('demoList.addTooltip')}>
            <IconButton size="2" variant="ghost" onClick={() => void addFiles()} disabled={busy} aria-label={t('demoList.add')}>
              <i aria-hidden="true" className="bi bi-plus-lg app-icon"  />
            </IconButton>
          </Tooltip>
          <Tooltip delayDuration={150} content={t('demoList.rescanTooltip')}>
            <IconButton size="2" variant="soft" onClick={() => void handleRefresh()} disabled={busy || refreshStatus === 'refreshing'} aria-busy={refreshStatus === 'refreshing'} aria-label={t('demoList.rescan')}>
              {refreshStatus === 'refreshing' ? <Spinner size="1" /> : <i aria-hidden="true" className="bi bi-arrow-clockwise app-icon"  />}
            </IconButton>
          </Tooltip>
        </Flex>
        <Popover.Root>
          <Tooltip delayDuration={150} content={t('ui.filter')}><Popover.Trigger><IconButton variant="ghost" className="filter-trigger" data-filtered={filtered} aria-label={t('ui.filter')}><i aria-hidden="true" className={'bi app-icon ' + (filtered ? 'bi-funnel-fill' : 'bi-funnel')} /></IconButton></Popover.Trigger></Tooltip>
          <Popover.Content width="320px" align="start"><Flex direction="column" gap="3">
        <TextField.Root size="2" placeholder={t('demoList.searchPlaceholder')} value={query} onChange={(e) => setQuery(e.target.value)}>
          <TextField.Slot>
            <i aria-hidden="true" className="bi bi-search app-icon"  />
          </TextField.Slot>
          {query && (
            <TextField.Slot side="right">
              <IconButton size="2" variant="ghost" color="gray" onClick={() => setQuery('')} aria-label={t('demoList.clearSearch')}>
                <i aria-hidden="true" className="bi bi-x-lg app-icon"  />
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
              <i aria-hidden="true" className="bi bi-x-lg app-icon"  />
            </IconButton>
          )}
        </Flex>

          </Flex></Popover.Content>
        </Popover.Root>
      </Flex>, toolbar)}

      <div ref={scrollRef} className="demo-scroll">
        {visible.length > 0 ? (
          <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
            {virtualizer.getVirtualItems().map((row) => {
              const d = visible[row.index]!;
              const exports = exportsByDemo.get(d.id);
              return (

                  <button key={d.id} type="button" aria-pressed={d.id === selectedId} className={'demo-item ' + (d.id === selectedId ? 'active' : '')} style={{ position: 'absolute', top: 0, left: 0, width: '100%', height: row.size, transform: 'translateY(' + row.start + 'px)' }} onClick={() => onSelect(d.id)}>
                    <span className="demo-chip">{d.mapName?.replace(/^de_/, '').slice(0, 2) ?? '—'}</span>
                    <span className="demo-copy">
                      <span className="demo-name"><Text weight="bold" truncate>{d.mapName ?? t('common.unknown')}</Text>
                        {d.summary && <span className="demo-score"><span>{d.summary.scoreA}</span><i>:</i><span>{d.summary.scoreB}</span></span>}
                      </span>
                      <span className="demo-date mono">{format(d.mtimeMs, 'yyyy-MM-dd HH:mm')}</span>
                      <span className="demo-states">
                        <span>{t('demoList.videos', { n: exports?.videos ?? 0 })}</span>
                        <span className="demo-job-states">
                        {d.status !== 'parsed' && <Badge size="1" color={STATUS_COLOR[d.status]}>{t(`demoList.status.${d.status}`)}</Badge>}
                        {exports && exports.running > 0 && <span className="queue-note">{t('demoList.exportJobs', { status: t('renders.status.running'), n: exports.running })}</span>}
                        {exports && exports.queued > 0 && <span className="queue-note">{t('demoList.exportJobs', { status: t('renders.status.queued'), n: exports.queued })}</span>}
                        {exports && exports.error > 0 && <Badge color="red"><i aria-hidden="true" className="bi bi-exclamation-triangle app-icon"  />{exports.error}</Badge>}
                        </span>
                      </span>
                    </span><i aria-hidden="true" className="bi bi-chevron-right demo-chevron app-icon" />
                  </button>
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
