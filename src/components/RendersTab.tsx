import { Spinner } from './Spinner.tsx';
import { useCallback, useEffect, useLayoutEffect, useRef, useState, type RefObject } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { AlertDialog, Badge, Box, Button, Callout, Card, DataList, Dialog, DropdownMenu, Flex, Heading, IconButton, Link, Progress, Text } from '@radix-ui/themes';
import { useTranslation } from 'react-i18next';
import type { TFunction } from 'i18next';
import { api, errorText, mb, type ParsedDemo, type RenderJob } from '../api.ts';
import { LogView } from './LogView.tsx';
import { fmtDateTime } from '../i18n/index.ts';

function initializePreview(video: HTMLVideoElement | null) {
  if (video) video.volume = 0.15;
}
const COLOR: Record<RenderJob['status'], 'gray' | 'amber' | 'green' | 'red'> = { queued: 'amber', running: 'amber', done: 'green', error: 'red', cancelled: 'gray' };
const STAGES = ['starting', 'recording', 'encoding'] as const;
const isStage = (s: string): s is (typeof STAGES)[number] => (STAGES as readonly string[]).includes(s);

/** "[3/5] recording …" or "encoding 2/4" → 0..1, undefined when the stage carries no counter. */
function progressOf(job: RenderJob): number | undefined {
  const m = job.stage?.match(/(\d+)\s*\/\s*(\d+)/);
  if (!m) return undefined;
  const cur = Number(m[1]);
  const total = Number(m[2]);
  return total > 0 ? Math.min(1, cur / total) : undefined;
}

function duration(job: RenderJob, t: TFunction, now: number): string | undefined {
  if (!job.startedAt) return undefined;
  const end = job.finishedAt ? new Date(job.finishedAt) : new Date(now);
  const sec = Math.max(0, Math.round((end.getTime() - new Date(job.startedAt).getTime()) / 1000));
  return sec < 60 ? t('common.seconds', { n: sec }) : t('common.minutesSeconds', { m: Math.floor(sec / 60), s: sec % 60 });
}

function JobCard({ job, parsed, onChanged }: { job: RenderJob; parsed: ParsedDemo; onChanged: () => Promise<void> }) {
  const { t } = useTranslation();
  const [logOpen, setLogOpen] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const titleOf = (id?: string) => parsed.highlights.find((h) => h.id === id)?.title;
  const act = (fn: () => Promise<unknown>) => () => void fn().then(onChanged).catch((e) => alert(errorText(e)));
  const active = job.status === 'queued' || job.status === 'running';
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    if (job.status !== 'running' || job.finishedAt) return;
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [job.status, job.finishedAt]);
  const pct = progressOf(job);
  const total = job.outputs.reduce((s, o) => s + o.bytes, 0);
  const revealTarget = job.outputs[0]?.file;
  const hasOptions = job.options.trueView || !job.options.hud || !job.options.crosshair || !job.options.radar || !job.options.killFeed || !job.options.viewmodel || !job.options.tracers || job.options.chat || job.options.xray || job.options.voice;

  return (
    <Card className="job-card">
      <Flex align="center" gap="3" wrap="wrap" className="job-header">
        <Badge color={COLOR[job.status]} size="2">
          {t(`renders.status.${job.status}`)}
        </Badge>
        <Heading data-text-role="body-heading" size="3">{fmtDateTime(job.createdAt)}</Heading>
        <Text size="2" color="gray">
          {t('renders.summary', { n: job.highlightIds.length })} · {job.options.maxSizeMb ? t('renders.sizeLimit', { mb: job.options.maxSizeMb }) : t('renders.noSizeLimit')} · {job.options.height}p{job.options.fps} · {job.options.codec}
        </Text>
        <Box style={{ flex: 1 }} />
        {active && (
          <Button size="1" variant="soft" color="red" onClick={act(() => api.cancel(job.id))}>
            {t('common.cancel')}
          </Button>
        )}
        {revealTarget && (
          <Button size="1" variant="soft" onClick={() => void api.reveal(revealTarget)}>
            <i aria-hidden="true" className="bi bi-folder2-open app-icon"  /> {t('renders.openFolder')}
          </Button>
        )}
        <DropdownMenu.Root>
          <DropdownMenu.Trigger>
            <IconButton size="1" variant="soft" aria-label={t('common.more')}>
              <i aria-hidden="true" className="bi bi-three-dots app-icon"  />
            </IconButton>
          </DropdownMenu.Trigger>
          <DropdownMenu.Content align="end">
            <DropdownMenu.Item onSelect={() => setLogOpen(true)}>
              <i aria-hidden="true" className="bi bi-file-earmark-text app-icon"  /> {t('renders.viewLog')}
            </DropdownMenu.Item>
            {!active && (
              <>
                <DropdownMenu.Separator />
                <DropdownMenu.Item color="red" onSelect={() => setConfirmDelete(true)}>
                  {t('renders.deleteJob')}
                </DropdownMenu.Item>
              </>
            )}
          </DropdownMenu.Content>
        </DropdownMenu.Root>
      </Flex>

      {active && (
        <Box className="job-content">
          <Flex justify="between" align="center" mb={pct !== undefined ? '1' : '0'}>
            <Flex align="center" gap="2">
              <Spinner size="1" />
              <Text size="2" color="amber">
                {job.stage ? (isStage(job.stage) ? t(`renders.stage.${job.stage}`) : job.stage) : job.status === 'queued' ? t('renders.waitingPrevious') : t('renders.starting')}
              </Text>
              {job.log.length > 0 && (
                <Text size="1" color="gray" truncate style={{ maxWidth: 520 }}>
                  {job.log[job.log.length - 1]}
                </Text>
              )}
            </Flex>
            <Text size="1" color="gray">
              {duration(job, t, now)}
            </Text>
          </Flex>
          {pct !== undefined && <Progress value={Math.round(pct * 100)} color="amber" />}
        </Box>
      )}

      {job.error && (
        <Callout.Root color="red" size="1" mt="3">
          <Callout.Text className="selectable">{job.error}</Callout.Text>
        </Callout.Root>
      )}

      {job.outputs.length > 0 && (
        <div className="job-outputs">
          {/* same small preview for every file (merged video included); the player is one click away */}
          <div className="job-previews">
            {job.outputs.map((o) => {
              const title = o.isFinal ? t('renders.merged') : (titleOf(o.highlightId) ?? o.title);
              return (
                <Box key={o.file} style={{ minWidth: 0 }}>
                  <video ref={initializePreview} controls preload="metadata" src={api.fileSrc(o.file)} />
                  <Text as="div" size="2" mt="1" truncate title={title}>
                    {title}
                  </Text>
                  <Text as="div" size="1" color="gray">
                    {mb(o.bytes)} ·{' '}
                    <Link size="1" href="#" onClick={(e) => (e.preventDefault(), void api.open(o.file))}>
                      {t('renders.openInPlayer')} <i aria-hidden="true" className="bi bi-box-arrow-up-right app-icon" style={{ verticalAlign: '-2px' }} />
                    </Link>
                  </Text>
                </Box>
              );
            })}
          </div>
          <DataList.Root size="1" className="job-details">
            <DataList.Item>
              <DataList.Label minWidth="0" style={{ flexBasis: 90 }}>{t('renders.files')}</DataList.Label>
              <DataList.Value>{job.outputs.length}</DataList.Value>
            </DataList.Item>
            <DataList.Item>
              <DataList.Label minWidth="0" style={{ flexBasis: 90 }}>{t('renders.totalSize')}</DataList.Label>
              <DataList.Value>{mb(total)}</DataList.Value>
            </DataList.Item>
            <DataList.Item>
              <DataList.Label minWidth="0" style={{ flexBasis: 90 }}>{t('renders.elapsed')}</DataList.Label>
              <DataList.Value>{duration(job, t, now) ?? '—'}</DataList.Value>
            </DataList.Item>
            <DataList.Item>
              <DataList.Label minWidth="0" style={{ flexBasis: 90 }}>{t('common.resolution')}</DataList.Label>
              <DataList.Value>
                {job.options.width}×{job.options.height} @ {job.options.fps}
              </DataList.Value>
            </DataList.Item>
            {hasOptions && <DataList.Item>
              <DataList.Label minWidth="0" style={{ flexBasis: 90 }}>{t('renders.options')}</DataList.Label>
              <DataList.Value>
                <Flex gap="1" wrap="wrap">
                  {job.options.trueView && <Badge size="1">TrueView</Badge>}
                  {!job.options.hud && <Badge size="1">{t('renders.badge.noHud')}</Badge>}
                  {job.options.hud && !job.options.crosshair && <Badge size="1">{t('renders.badge.noCrosshair')}</Badge>}
                  {!job.options.radar && <Badge size="1">{t('renders.badge.noRadar')}</Badge>}
                  {!job.options.killFeed && <Badge size="1">{t('renders.badge.noKillFeed')}</Badge>}
                  {!job.options.viewmodel && <Badge size="1">{t('renders.badge.noViewmodel')}</Badge>}
                  {!job.options.tracers && <Badge size="1">{t('renders.badge.noTracers')}</Badge>}
                  {job.options.chat && <Badge size="1">{t('renders.badge.chat')}</Badge>}
                  {job.options.xray && <Badge size="1">X-ray</Badge>}
                  {job.options.voice && <Badge size="1">{t('renders.badge.voice')}</Badge>}
                </Flex>
              </DataList.Value>
            </DataList.Item>}
            <DataList.Item>
              <DataList.Label minWidth="0" style={{ flexBasis: 90 }}>{t('renders.location')}</DataList.Label>
              <DataList.Value>
                <Text className="mono selectable" style={{ overflowWrap: 'anywhere' }}>
                  {job.outputs[0]?.file.replace(/[\\/][^\\/]+$/, '')}
                </Text>
              </DataList.Value>
            </DataList.Item>
          </DataList.Root>
        </div>
      )}

      <Dialog.Root open={logOpen} onOpenChange={setLogOpen}>
        <Dialog.Content maxWidth="900px">
          <Dialog.Title>{t('renders.logTitle')}</Dialog.Title>
          <Dialog.Description size="2" color="gray">
            {fmtDateTime(job.createdAt)} · {t(`renders.status.${job.status}`)}
          </Dialog.Description>
          <LogView lines={job.log} empty={t('renders.logEmpty')} />
          <Flex justify="end" mt="3">
            <Dialog.Close>
              <Button variant="soft">{t('common.close')}</Button>
            </Dialog.Close>
          </Flex>
        </Dialog.Content>
      </Dialog.Root>

      <AlertDialog.Root open={confirmDelete} onOpenChange={setConfirmDelete}>
        <AlertDialog.Content maxWidth="440px">
          <AlertDialog.Title>{t('renders.deleteTitle')}</AlertDialog.Title>
          <AlertDialog.Description size="2">{t('renders.deleteBody', { n: job.outputs.length })}</AlertDialog.Description>
          <Flex gap="3" mt="4" justify="end">
            <AlertDialog.Cancel>
              <Button variant="soft" color="gray">
                {t('common.cancel')}
              </Button>
            </AlertDialog.Cancel>
            <AlertDialog.Action>
              <Button color="red" onClick={act(() => api.deleteJob(job.id))}>
                {t('common.delete')}
              </Button>
            </AlertDialog.Action>
          </Flex>
        </AlertDialog.Content>
      </AlertDialog.Root>
    </Card>
  );
}

export function RendersTab({ jobs, parsed, onChanged, scrollRef }: { jobs: RenderJob[]; parsed: ParsedDemo; onChanged: () => Promise<void>; scrollRef: RefObject<HTMLDivElement | null> }) {
  const { t } = useTranslation();
  const listRef = useRef<HTMLDivElement>(null);
  const [scrollMargin, setScrollMargin] = useState(0);
  const hasJobs = jobs.length > 0;
  const getItemKey = useCallback((index: number) => jobs[index]!.id, [jobs]);
  const virtualizer = useVirtualizer({
    count: jobs.length,
    getScrollElement: () => scrollRef.current,
    getItemKey,
    estimateSize: (index) => jobs[index]!.outputs.length > 0 ? 320 : 110,
    overscan: 3,
    gap: 12,
    scrollMargin,
  });

  useLayoutEffect(() => {
    const scroll = scrollRef.current;
    const list = listRef.current;
    if (!scroll || !list) return;
    let width = scroll.clientWidth;
    const measureLayout = () => {
      // Account for the tab body's padding without introducing a second scrollbar.
      setScrollMargin(list.getBoundingClientRect().top - scroll.getBoundingClientRect().top + scroll.scrollTop - scroll.clientTop);
      if (scroll.clientWidth !== width) {
        width = scroll.clientWidth;
        // Off-screen measurements also become stale when cards reflow at a new width.
        virtualizer.measure();
      }
    };
    measureLayout();
    const observer = new ResizeObserver(measureLayout);
    observer.observe(scroll);
    return () => observer.disconnect();
  }, [hasJobs, scrollRef, virtualizer]);

  if (!hasJobs) {
    return <Text as="p" size="2" color="gray">{t('renders.empty')}</Text>;
  }
  return (
    <><Flex align="center" gap="3" mb="3"><Heading data-text-role="subtitle" size="3">{t('demoView.tabs.videos')}</Heading><Text size="1" color="gray">{t('demoList.videos', { n: jobs.reduce((n, job) => n + job.outputs.length, 0) })} · {mb(jobs.reduce((n, job) => n + job.outputs.reduce((bytes, output) => bytes + output.bytes, 0), 0))}</Text></Flex>
    <div ref={listRef} style={{ height: virtualizer.getTotalSize(), position: 'relative', overflowAnchor: 'none' }}>
      {virtualizer.getVirtualItems().map((row) => (
        <div
          key={row.key}
          data-index={row.index}
          ref={virtualizer.measureElement}
          style={{ position: 'absolute', top: 0, left: 0, width: '100%', transform: `translateY(${row.start - scrollMargin}px)` }}
        >
          <JobCard job={jobs[row.index]!} parsed={parsed} onChanged={onChanged} />
        </div>
      ))}
    </div></>
  );
}
