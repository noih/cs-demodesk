import { Spinner } from './Spinner.tsx';
import { useEffect, useRef, useState } from 'react';
import { AlertDialog, Badge, Box, Button, Callout, DropdownMenu, Flex, Heading, IconButton, Tabs, Text, Tooltip } from '@radix-ui/themes';
import { Trans, useTranslation } from 'react-i18next';
import { api, errorText, mb, type DemoMeta, type ParsedDemo, type RenderJob, type Status } from '../api.ts';
import { fmtDate } from '../i18n/index.ts';
import { HighlightsTab } from './HighlightsTab.tsx';
import { PlayersTab } from './PlayersTab.tsx';
import { RendersTab } from './RendersTab.tsx';
import { ChartsTab } from './ChartsTab.tsx';
import { ReplayTab } from './ReplayTab.tsx';

export function DemoView({ meta, jobs, status, onChanged, onRemoved, requestedTab, selectionRequest, onSetup }: { onSetup: () => void; requestedTab: string; selectionRequest: number; meta: DemoMeta; jobs: RenderJob[]; status?: Status; onChanged: () => Promise<void>; onRemoved: () => void }) {
  const { t } = useTranslation();
  const [parsed, setParsed] = useState<ParsedDemo>();
  const [loading, setLoading] = useState(true);
  const [parseRequested, setParseRequested] = useState(false);
  const parsing = parseRequested || meta.status === 'parsing';
  const [tab, setTab] = useState(requestedTab);
  useEffect(() => { setTab(requestedTab); }, [requestedTab, selectionRequest]);
  const tabScrollRef = useRef<HTMLDivElement>(null);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [loadError, setLoadError] = useState<string>();

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
    void fn()
      .then(onChanged)
      .catch((e) => alert(errorText(e)));
  const reparse = async () => {
    if (parsing) return;
    setParseRequested(true);
    try {
      await api.parse(meta.id);
      await onChanged();
    } catch (e) {
      alert(errorText(e));
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
      alert(errorText(e));
    } finally {
      setConfirmRemove(false);
    }
  };
  const videoCount = jobs.reduce((count, job) => count + job.outputs.length, 0);
  const activeJobs = jobs.filter((j) => j.status === 'running' || j.status === 'queued').length;

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
                <DropdownMenu.Item disabled={meta.status !== 'parsed'} onSelect={run(() => api.clearAnalysis(meta.id))}>
                  {t('demoView.clearAnalysis')}
                </DropdownMenu.Item>
                <DropdownMenu.Separator />
                <DropdownMenu.Item color="red" disabled={meta.status === 'parsing'} onSelect={() => setConfirmRemove(true)}>
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
          <Tabs.List className="demo-tabs">
            <Tabs.Trigger value="players"><i aria-hidden="true" className="bi bi-people app-icon" />{t('demoView.tabs.players')}</Tabs.Trigger>
            <Tabs.Trigger value="charts"><i aria-hidden="true" className="bi bi-graph-up app-icon" />{t('demoView.tabs.charts')}</Tabs.Trigger>
            <Tabs.Trigger value="highlights">
              <i aria-hidden="true" className="bi bi-stars app-icon" />{t('demoView.tabs.highlights')}
              <Badge ml="2" variant="soft" color="gray">
                {parsed.highlights.length}
              </Badge>
            </Tabs.Trigger>
            <Tabs.Trigger value="renders">
              <i aria-hidden="true" className="bi bi-camera-video app-icon" />{t('demoView.tabs.videos')}
              <Badge ml="2" variant="soft" color="gray">
                {videoCount}
              </Badge>
              {activeJobs > 0 && (
                <Badge ml="2" variant="soft" color="amber">
                  {t('demoView.activeJobs', { n: activeJobs })}
                </Badge>
              )}
            </Tabs.Trigger>
            <Tabs.Trigger value="2d"><i aria-hidden="true" className="bi bi-map app-icon" />{t('demoView.tabs.replay')}</Tabs.Trigger>
          </Tabs.List>
          <Box ref={tabScrollRef} className="tab-body">
            <Tabs.Content value="highlights">
              <HighlightsTab onSetup={onSetup} meta={meta} parsed={parsed} status={status} onRendered={() => setTab('renders')} />
            </Tabs.Content>
            <Tabs.Content value="players">
              <PlayersTab parsed={parsed} />
            </Tabs.Content>
            <Tabs.Content value="charts">
              <ChartsTab parsed={parsed} />
            </Tabs.Content>
            <Tabs.Content value="renders">
              <RendersTab jobs={jobs} parsed={parsed} onChanged={onChanged} scrollRef={tabScrollRef} />
            </Tabs.Content>
            <Tabs.Content value="2d" style={{ height: '100%' }}>
              <ReplayTab onSetup={onSetup} meta={meta} parsed={parsed} />
            </Tabs.Content>
          </Box>
        </Tabs.Root>
      )}

      <AlertDialog.Root open={confirmRemove} onOpenChange={setConfirmRemove}>
        <AlertDialog.Content maxWidth="480px">
          <AlertDialog.Title>{t('demoView.deleteTitle')}</AlertDialog.Title>
          <AlertDialog.Description size="2">
            <Trans i18nKey="demoView.deleteBody" components={{ path: <span className="mono selectable">{meta.path}</span> }} />
          </AlertDialog.Description>
          <Flex gap="3" mt="4" justify="end">
            <AlertDialog.Cancel>
              <Button variant="soft" color="gray">
                {t('common.cancel')}
              </Button>
            </AlertDialog.Cancel>
            <AlertDialog.Action>
              <Button color="red" onClick={() => void doRemove()}>
                {t('demoView.deleteConfirm')}
              </Button>
            </AlertDialog.Action>
          </Flex>
        </AlertDialog.Content>
      </AlertDialog.Root>
    </Flex>
  );
}
