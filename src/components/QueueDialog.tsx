import { Tooltip } from '@radix-ui/themes';
import { Spinner } from './Spinner.tsx';
import { useCallback, useEffect, useRef, useState } from 'react';
import { Badge, Button, Dialog, Flex, IconButton, Text } from '@radix-ui/themes';
import { useTranslation } from 'react-i18next';
import { api, errorText, type DemoMeta, type RenderJob } from '../api.ts';
import { fmtDateTime } from '../i18n/index.ts';

type ClipInfo = { titles: Map<string, string>; error?: string };
type LoadClips = (demoId: string) => Promise<ClipInfo>;

function QueueClips({ job, loadClips }: { job: RenderJob; loadClips: LoadClips }) {
  const ref = useRef<HTMLDivElement>(null);
  const [info, setInfo] = useState<ClipInfo>();
  useEffect(() => {
    const element = ref.current;
    if (!element) return;
    let alive = true;
    const observer = new IntersectionObserver(entries => {
      if (!entries.some(entry => entry.isIntersecting)) return;
      observer.disconnect();
      void loadClips(job.demoId).then(result => { if (alive) setInfo(result); });
    }, { root: element.closest('.queue-list') });
    observer.observe(element);
    return () => { alive = false; observer.disconnect(); };
  }, [job.demoId, loadClips]);
  return <div ref={ref} className="queue-clips">
    {info?.error && <Text color="red">{info.error}</Text>}
    {!info && <Spinner />}
    <ul>{job.highlightIds.map(id => <li key={id}>{info?.titles.get(id) ?? id}</li>)}</ul>
  </div>;
}

function QueueList({ jobs, demos, onSelect }: { jobs: RenderJob[]; demos: DemoMeta[]; onSelect: (id: string) => void }) {
  const { t } = useTranslation();
  // Share in-flight reads and retain only titles for this dialog opening.
  const cache = useRef(new Map<string, Promise<ClipInfo>>());
  const loadClips = useCallback<LoadClips>(demoId => {
    let request = cache.current.get(demoId);
    if (!request) {
      request = api.demo(demoId)
        .then(r => ({ titles: new Map(r.parsed?.highlights.map(h => [h.id, h.title]) ?? []) }))
        .catch(e => ({ titles: new Map<string, string>(), error: errorText(e) }));
      cache.current.set(demoId, request);
    }
    return request;
  }, []);
  return <div className="queue-list">{jobs.length ? jobs.map(job => {
    const demo = demos.find(d => d.id === job.demoId);
    return <section className="queue-item" key={job.id}>
      <Flex gap="2" align="center" wrap="wrap">
        <Badge color="amber">{t(`renders.status.${job.status}`)}</Badge>
        <Text weight="medium" className="queue-demo-name">{demo?.mapName ?? demo?.name ?? job.demoId}</Text>
      </Flex>
      <Text as="p" size="1" color="gray" className="queue-meta">{fmtDateTime(job.createdAt)} · {t('renders.summary', { n: job.highlightIds.length })} · {job.options.height}p{job.options.fps}</Text>
      {job.stage && <Text as="p" size="2" className="queue-stage">{job.stage.replace(/\b(starting|recording|encoding)\b/g, stage => t('renders.stage.' + stage, { defaultValue: stage }))}</Text>}
      <QueueClips job={job} loadClips={loadClips} />
      <Flex gap="2" wrap="wrap" justify="end" className="queue-actions">
        <Button size="2" variant="outline" disabled={!demo} onClick={() => onSelect(job.demoId)}>{t('ui.goToDemo')}</Button>
      </Flex>
    </section>;
  }) : <Text>{t('ui.queueEmpty')}</Text>}</div>;
}
export function QueueDialog({ jobs, demos, onSelect }: { jobs: RenderJob[]; demos: DemoMeta[]; onSelect: (id: string) => void }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);

  const active = jobs.filter(j => j.status === 'running' || j.status === 'queued');

  return <Dialog.Root open={open} onOpenChange={setOpen}>
    <Tooltip delayDuration={150} content={t('ui.queue')}><Dialog.Trigger>{active.length ? <Button variant="soft" color="gray" className="queue-trigger"><span className="queue-dot" />{t('ui.queueCount', { running: active.filter(j => j.status === 'running').length, queued: active.filter(j => j.status === 'queued').length })}</Button> : <IconButton variant="ghost" className="queue-trigger" aria-label={t('ui.queue')} ><i aria-hidden="true" className="bi bi-list-task app-icon" /></IconButton>}</Dialog.Trigger></Tooltip>
    <Dialog.Content maxWidth="720px" aria-describedby={undefined}><Dialog.Title>{t('ui.queue')}</Dialog.Title>
      <QueueList jobs={active} demos={demos} onSelect={id => { onSelect(id); setOpen(false); }} />
      <Flex justify="end" mt="4"><Dialog.Close><Button variant="soft">{t('common.close')}</Button></Dialog.Close></Flex>
    </Dialog.Content>
  </Dialog.Root>;
}
