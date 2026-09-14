import { useNotify, ConfirmDialog } from './Notifications.tsx';
import { Spinner } from './Spinner.tsx';
import { useEffect, useRef, useState } from 'react';
import { Badge, Box, Button, Callout, DropdownMenu, Flex, Heading, IconButton, Tabs, Text, Tooltip } from '@radix-ui/themes';
import { Trans, useTranslation } from 'react-i18next';
import { api, errorText, mb, type AnalysisJob, type Assessment, type DemoMeta, type ParsedDemo, type RenderJob, type Status } from '../api.ts';
import { fmtDate } from '../i18n/index.ts';
import { HighlightsTab } from './HighlightsTab.tsx';
import { ScoringTab } from './ScoringTab.tsx';
import { PlayersTab } from './PlayersTab.tsx';
import { RendersTab } from './RendersTab.tsx';
import { ChartsTab } from './ChartsTab.tsx';
import { ReplayTab } from './ReplayTab.tsx';

export function DemoView({ analysisJobs, meta, jobs, status, onChanged, onRemoved, requestedTab, selectionRequest, onSetup }: { analysisJobs: AnalysisJob[]; onSetup: (target: 'render' | 'replay') => void; requestedTab: string; selectionRequest: number; meta: DemoMeta; jobs: RenderJob[]; status?: Status; onChanged: () => Promise<void>; onRemoved: () => void }) {
  const { t } = useTranslation();
  const notify = useNotify();
  const [parsed, setParsed] = useState<ParsedDemo>();
  const [loading, setLoading] = useState(true);
  const [parseRequested, setParseRequested] = useState(false);
  const parsing = parseRequested || meta.status === 'parsing';
  const [tab, setTab] = useState(requestedTab);
  useEffect(() => { setTab(requestedTab); }, [requestedTab, selectionRequest]);
  const tabScrollRef = useRef<HTMLDivElement>(null);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [dataRevision, setDataRevision] = useState(0);
  const [analysisRecords, setAnalysisRecords] = useState<Record<string, Assessment[]>>();
  const [historyError, setHistoryError] = useState<string>();
  const [loadError, setLoadError] = useState<string>();

  const analysisJob=analysisJobs.filter(job=>job.demoId===meta.id).sort((a,b)=>b.sequence-a.sequence)[0];
  useEffect(() => {
    let alive = true;
    setAnalysisRecords(undefined);
    setHistoryError(undefined);
    void api.scoringHistory(meta.id).then(records => {
      if (alive) setAnalysisRecords(records);
    }).catch(error => { if (alive) setHistoryError(errorText(error)); });
    return () => { alive = false; };
  }, [meta.id, meta.parsedAt, analysisJob?.finishedAt, dataRevision]);
  const hasAnalysis = Object.values(analysisRecords ?? {}).some(records => records.length > 0);
  const [submitting,setSubmitting]=useState(false);
  const [scoringError,setScoringError]=useState<string>();
  const scoringBusy=submitting || analysisJob?.status==='queued' || analysisJob?.status==='running';
  const queuePosition=analysisJob?.status==='queued' ? analysisJobs.filter(job=>job.status==='queued').sort((a,b)=>a.sequence-b.sequence).findIndex(job=>job.id===analysisJob.id)+1 : undefined;
  const scoreMatch=async()=>{
    if (scoringBusy) return;
    setSubmitting(true);setScoringError(undefined);setTab('scoring');
    try {await api.scoreMatch(meta.id);await onChanged();}
    catch(error){setScoringError(errorText(error));}
    finally{setSubmitting(false);}
  };

  useEffect(() => {
    let alive = true;
    setLoading(true);
    setLoadError(undefined);
    api
      .demo(meta.id)
      .then((r) => alive && setParsed(r.parsed))
      .catch((e) => alive && setLoadError(errorText(e)))
      .finally(() => { if (alive) setLoading(false); });
    return () => {
      alive = false;
    };
  }, [meta.id, meta.status, meta.parsedAt]);

  const run = (fn: () => Promise<unknown>) => () =>
    void fn().then(onChanged).catch(error => notify(errorText(error)));
  const deleteData = (fn: () => Promise<unknown>, analysisChanged = true) => () => {
    if (deleting) return;
    setDeleting(true);
    void (async () => {
      try {
        await fn();
        if (analysisChanged) {
          setDataRevision(value => value + 1);
          setScoringError(undefined);
        }
      } catch (error) { notify(errorText(error)); }
      finally {
        try { await onChanged(); } catch (error) { notify(errorText(error)); }
        setDeleting(false);
      }
    })();
  };
  const reparse = async () => {
    if (parsing) return;
    setParseRequested(true);
    try {
      await api.parse(meta.id);
      await onChanged();
    } catch (e) {
      notify(errorText(e));
    } finally {
      setParseRequested(false);
    }
  };
  const doRemove = async () => {
    try {
      await api.removeDemo(meta.id);
      onRemoved();
      await onChanged();
    } catch (e) {
      notify(errorText(e));
    } finally {
      setConfirmRemove(false);
    }
  };
  const videoCount = jobs.reduce((count, job) => count + job.outputs.length, 0);
  const runningJobs = jobs.filter((j) => j.status === 'running').length;
  const queuedJobs = jobs.filter((j) => j.status === 'queued').length;

  if (loading && !parsed) {
    return (
      <Flex align="center" justify="center" gap="2" style={{ height: '100%' }} aria-busy="true">
        <Spinner />
        <Text color="gray">{t('replay.preparing')}</Text>
      </Flex>
    );
  }

  return (
    <Flex direction="column" gap="3" style={{ height: '100%' }}>
      <Flex direction="column" gap="2" className="demo-heading">
        <Flex justify="between" align="center" gap="3">
          <Heading data-text-role="title" size="6" truncate style={{ minWidth: 0 }}>
            {parsed ? parsed.info.mapName : meta.name.replace(/\.dem$/i, '')}
          </Heading>
          {parsed && <div className="match-score"><div className="team-a"><small>{t('common.teamA')}</small><strong>{parsed.score.A}</strong></div><div className="team-b"><small>{t('common.teamB')}</small><strong>{parsed.score.B}</strong></div></div>}
          <Flex gap="2" style={{ flex: 'none' }}>
            {(parsed || meta.status === 'parsed' || parsing) && (
              <Tooltip delayDuration={150} content={t('demoView.reparse')}>
                <IconButton variant="soft" onClick={() => void reparse()} disabled={parsing} aria-busy={parsing} aria-label={t('demoView.reparse')}>
                  {parsing ? <Spinner size="1" /> : <i aria-hidden="true" className="bi bi-arrow-clockwise app-icon" />}
                </IconButton>
              </Tooltip>
            )}
            <DropdownMenu.Root>
              <DropdownMenu.Trigger>
                <IconButton variant="soft" aria-label={t('common.more')}>
                  <i aria-hidden="true" className="bi bi-three-dots app-icon"  />
                </IconButton>
              </DropdownMenu.Trigger>
              <DropdownMenu.Content align="end">
                <DropdownMenu.Item onSelect={() => void api.reveal(meta.path)}>{t('common.openInExplorer')}</DropdownMenu.Item>
                <DropdownMenu.Item disabled={deleting || jobs.length === 0 || runningJobs > 0 || queuedJobs > 0} onSelect={deleteData(async () => {
                  for (const job of jobs) await api.deleteJob(job.id);
                }, false)}>{t('demoView.deleteVideos')}</DropdownMenu.Item>
                <DropdownMenu.Item disabled={!hasAnalysis || deleting || scoringBusy || parsing} onSelect={deleteData(() => api.clearMatchAnomaly(meta.id))}>{t('demoView.deleteAnomaly')}</DropdownMenu.Item>
                <DropdownMenu.Item disabled={deleting || parsing || scoringBusy || runningJobs > 0 || queuedJobs > 0 || meta.status !== 'parsed'} onSelect={deleteData(() => api.clearAnalysis(meta.id))}>
                  {t('demoView.clearAnalysis')}
                </DropdownMenu.Item>
                <DropdownMenu.Separator />
                <DropdownMenu.Item color="red" disabled={deleting || parsing} onSelect={() => setConfirmRemove(true)}>
                  {t('demoView.deleteDemo')}
                </DropdownMenu.Item>
              </DropdownMenu.Content>
            </DropdownMenu.Root>
          </Flex>
        </Flex>
        <Flex align="center" gap="2" wrap="wrap">
          {parsed && (
            <>
              <Text size="1" color="gray">
                {t('demoView.rounds', { count: parsed.rounds.length })}
              </Text>
              <Text size="1" color="gray">
                ·
              </Text>
            </>
          )}
          <Text size="1" color="gray">
            {meta.name}
          </Text>
          <Text size="1" color="gray">
            · {fmtDate(meta.mtimeMs)} · {mb(meta.bytes)}
          </Text>
        </Flex>
      </Flex>

      {(!parsed || parsing) && (
        <Flex align="center" justify="center" direction="column" gap="4" style={{ flex: 1 }}>
          {parsing ? (
            <>
              <Spinner size="3" />
              <Text color="gray">{t('demoView.parsing')}</Text>
            </>
          ) : (
            <>
              {(meta.status === 'error' || loadError) && (
                <Callout.Root color="red" size="1" style={{ maxWidth: 640 }}>
                  <Callout.Text className="selectable">{loadError ?? meta.error}</Callout.Text>
                </Callout.Root>
              )}
              <Button size="3" onClick={run(() => api.parse(meta.id))}>
                <i aria-hidden="true" className="bi bi-arrow-clockwise app-icon"  /> {meta.status === 'error' ? t('demoView.reparse') : t('demoView.parse')}
              </Button>
            </>
          )}
        </Flex>
      )}

      {parsed && !parsing && (
        <Tabs.Root value={tab} onValueChange={setTab} style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
          <div className="demo-tabs-bar"><Tabs.List className="demo-tabs">
            <Tabs.Trigger value="players" title={t('demoView.tabs.players')} aria-label={t('demoView.tabs.players')}><i aria-hidden="true" className="bi bi-people app-icon" /><span className="demo-tab-label">{t('demoView.tabs.players')}</span></Tabs.Trigger>
            <Tabs.Trigger value="charts" title={t('demoView.tabs.charts')} aria-label={t('demoView.tabs.charts')}><i aria-hidden="true" className="bi bi-graph-up app-icon" /><span className="demo-tab-label">{t('demoView.tabs.charts')}</span></Tabs.Trigger>
            <Tabs.Trigger value="scoring" title={t('scoring.title')} aria-label={t('scoring.title')}><i aria-hidden="true" className="bi bi-shield-check app-icon" /><span className="demo-tab-label">{t('scoring.title')}</span></Tabs.Trigger>
            <Tabs.Trigger value="highlights" title={t('demoView.tabs.highlights')} aria-label={t('demoView.tabs.highlights')}>
              <i aria-hidden="true" className="bi bi-stars app-icon" /><span className="demo-tab-label">{t('demoView.tabs.highlights')}</span>
              <Badge ml="2" variant="soft" color="gray">
                {parsed.highlights.length}
              </Badge>
            </Tabs.Trigger>
            <Tabs.Trigger value="renders" title={t('demoView.tabs.videos')} aria-label={t('demoView.tabs.videos')}>
              <i aria-hidden="true" className="bi bi-camera-video app-icon" /><span className="demo-tab-label">{t('demoView.tabs.videos')}</span>
              <Badge ml="2" variant="soft" color="gray">
                {videoCount}
              </Badge>
              {queuedJobs > 0 && (
                <Tooltip content={`${t('renders.status.queued')}: ${queuedJobs}`}>
                  <Badge ml="2" variant="soft" color="gray" aria-label={`${t('renders.status.queued')}: ${queuedJobs}`}>
                    <i aria-hidden="true" className="bi bi-hourglass-split" />{queuedJobs}
                  </Badge>
                </Tooltip>
              )}
              {runningJobs > 0 && (
                <Tooltip content={`${t('renders.status.running')}: ${runningJobs}`}>
                  <Badge ml="2" variant="soft" color="amber" aria-label={`${t('renders.status.running')}: ${runningJobs}`}>
                    <span aria-hidden="true" style={{ display: 'inline-flex', alignItems: 'center' }}><Spinner size="1" /></span>{runningJobs}
                  </Badge>
                </Tooltip>
              )}
            </Tabs.Trigger>
            <Tabs.Trigger value="2d" title={t('demoView.tabs.replay')} aria-label={t('demoView.tabs.replay')}><i aria-hidden="true" className="bi bi-map app-icon" /><span className="demo-tab-label">{t('demoView.tabs.replay')}</span></Tabs.Trigger>
          </Tabs.List></div>
          <Box ref={tabScrollRef} className="tab-body">
            <Tabs.Content value="highlights">
              <HighlightsTab onSetup={() => onSetup('render')} meta={meta} parsed={parsed} status={status} onRendered={() => setTab('renders')} />
            </Tabs.Content>
            <Tabs.Content value="players">
              <PlayersTab parsed={parsed} />
            </Tabs.Content>
            <Tabs.Content value="scoring"><ScoringTab onAnalyze={()=>void scoreMatch()} status={status} onSetup={()=>onSetup('render')} onRendered={()=>setTab('renders')} key={`${meta.id}:${analysisJob?.finishedAt ?? ''}:${dataRevision}`} players={analysisRecords ?? {}} parsed={parsed} busy={scoringBusy} step={analysisJob?.step ?? 1} job={analysisJob} queuePosition={queuePosition} error={scoringError || historyError}/></Tabs.Content>
            <Tabs.Content value="charts">
              <ChartsTab parsed={parsed} />
            </Tabs.Content>
            <Tabs.Content value="renders">
              <RendersTab jobs={jobs} parsed={parsed} onChanged={onChanged} scrollRef={tabScrollRef} />
            </Tabs.Content>
            <Tabs.Content value="2d" style={{ height: '100%' }}>
              <ReplayTab onSetup={() => onSetup('replay')} meta={meta} parsed={parsed} />
            </Tabs.Content>
          </Box>
        </Tabs.Root>
      )}

      <ConfirmDialog open={confirmRemove} onOpenChange={setConfirmRemove} title={t('demoView.deleteTitle')}
        description={<Trans i18nKey="demoView.deleteBody" components={{ path: <span className="mono selectable">{meta.path}</span> }} />}
        confirmLabel={t('demoView.deleteConfirm')} onConfirm={() => void doRemove()} />
    </Flex>
  );
}
