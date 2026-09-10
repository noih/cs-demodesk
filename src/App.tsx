import { NotificationProvider, Toast } from './components/Notifications.tsx';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button, Flex, IconButton, Popover, Text, Tooltip } from '@radix-ui/themes';
import { api, errorText, type DemoMeta, type RenderJob, type Status } from './api.ts';
import { createAppSync } from './appSync.ts';
import { applyLanguage } from './i18n/index.ts';
import { StartupGate } from './components/StartupGate.tsx';
import { DemoList } from './components/DemoList.tsx';
import { DemoView } from './components/DemoView.tsx';
import { SettingsView } from './components/SettingsView.tsx';

import { useAppTheme } from './AppTheme.tsx';
import { AboutDialog } from './components/AboutDialog.tsx';
import { QueueDialog } from './components/QueueDialog.tsx';

export function App() {
  return <NotificationProvider><StartupGate><ReadyApp /></StartupGate></NotificationProvider>;
}

function ReadyApp() {
  const { t } = useTranslation();
  const theme = useAppTheme();
  const [fontSizeOpen, setFontSizeOpen] = useState(false);
  const [toolbar, setToolbar] = useState<HTMLDivElement | null>(null);
  const [selectionRequest, setSelectionRequest] = useState(0);
  const [requestedTab, setRequestedTab] = useState('players');
  const [status, setStatus] = useState<Status>();
  const [demos, setDemos] = useState<DemoMeta[]>([]);
  const [jobs, setJobs] = useState<RenderJob[]>([]);
  const [selectedId, setSelectedId] = useState<string>();
  const [showSettings, setShowSettings] = useState(false);
  const [toolsRequest, setToolsRequest] = useState(0);
  const [toolsTarget, setToolsTarget] = useState<'render' | 'replay'>('render');
  const showTools = (target: 'render' | 'replay') => { setToolsTarget(target); setShowSettings(true); setToolsRequest(n => n + 1); };
  const [error, setError] = useState<string>();

  const syncRef = useRef<ReturnType<typeof createAppSync> | undefined>(undefined);
  const rescan = useCallback(() => syncRef.current?.refresh() ?? Promise.resolve(false), []);
  const refresh = useCallback(async () => { await rescan(); }, [rescan]);

  useEffect(() => {
    void api.settings().then((s) => applyLanguage(s.settings.language)).catch(() => undefined);
    const sync = createAppSync(api, (snapshot) => {
      setStatus(snapshot.status);
      setDemos(snapshot.demos);
      setJobs(snapshot.jobs);
      setError(undefined);
    }, (ev) => {
      if (ev.type === 'demo-changed') setDemos((list) => upsert(list, ev.demo, (d) => d.id));
      if (ev.type === 'job-changed') setJobs((list) => {
        const next = upsert(list, ev.job, (j) => j.id);
        return next.length === list.length ? next : next.sort((a, b) => b.createdAt.localeCompare(a.createdAt));
      });
    }, (error) => setError(errorText(error)));
    syncRef.current = sync;
    void sync.refresh();
    return () => {
      syncRef.current = undefined;
      sync.dispose();
    };
  }, []);

  const selected = demos.find((d) => d.id === selectedId);


  return (
    <div className="layout">
      <header className="app-header">
        <div ref={setToolbar} className="header-tools" />
        <Flex align="center" gap="2" ml="auto">
          <QueueDialog jobs={jobs} demos={demos} onSelect={id => { setSelectedId(id); setShowSettings(false); setRequestedTab('renders'); setSelectionRequest(n => n + 1); }} />
          <Tooltip delayDuration={150} content={t(theme.appearance === 'dark' ? 'ui.light' : 'ui.dark')}><IconButton variant="ghost" aria-label={t(theme.appearance === 'dark' ? 'ui.light' : 'ui.dark')} onClick={theme.toggle}>{theme.appearance === 'dark' ? <i aria-hidden="true" className="bi bi-sun app-icon"  /> : <i aria-hidden="true" className="bi bi-moon app-icon" />}</IconButton></Tooltip>
          <Popover.Root open={fontSizeOpen} onOpenChange={setFontSizeOpen}>
            <Tooltip delayDuration={150} content={t('ui.fontSize')}><Popover.Trigger><Button variant="ghost" aria-label={t('ui.fontSize')}>Aa</Button></Popover.Trigger></Tooltip>
            <Popover.Content size="1">
          <div className="font-size-control" role="radiogroup" aria-label={t('ui.fontSize')}>
            {(['small', 'medium', 'large'] as const).map((size, index) => <label key={size} title={t(`ui.fontSize_${size}`)}>
              <input type="radio" name="font-size" value={size} checked={theme.fontSize === size} onClick={() => setFontSizeOpen(false)} onChange={() => theme.setFontSize(size)} aria-label={t(`ui.fontSize_${size}`)} />
              <span aria-hidden="true" style={{ fontSize: 14 + index * 3 }}>Aa</span>
            </label>)}
          </div>
            </Popover.Content>
          </Popover.Root>
          <Tooltip delayDuration={150} content={t('common.settings')}><IconButton variant="ghost" aria-label={t('common.settings')} aria-pressed={showSettings} color="gray" onClick={() => { setToolsRequest(0); setShowSettings(v => !v); }}><i aria-hidden="true" className="bi bi-gear app-icon" /></IconButton></Tooltip><AboutDialog />
        </Flex>
      </header>
      <aside className="sidebar">
        <DemoList
          demos={demos}
          jobs={jobs}
          selectedId={showSettings ? undefined : selectedId}
          toolbar={toolbar}
          selectionRequest={selectionRequest}
          onSelect={(id) => {
            setRequestedTab('players');
            setSelectedId(id);
            setShowSettings(false);
          }}
          onChanged={refresh}
          onRefresh={rescan}
        />

      </aside>
      {error && <Toast message={t('app.backendError', { error })} color="red" onDismiss={() => setError(undefined)}
        action={<Button size="1" variant="soft" onClick={() => { setToolsRequest(0); setShowSettings(true); }}>{t('common.settings')}</Button>} />}
      <main className="main">
        {showSettings ? (
          <SettingsView onChanged={refresh} toolsRequest={toolsRequest} toolsTarget={toolsTarget} />
        ) : selected ? (
          <DemoView onSetup={showTools} requestedTab={requestedTab} selectionRequest={selectionRequest} key={selected.id} meta={selected} jobs={jobs.filter((j) => j.demoId === selected.id)} status={status} onChanged={refresh} onRemoved={() => setSelectedId(undefined)} />
        ) : (
          <Flex align="center" justify="center" style={{ height: '100%' }}>
            <Flex direction="column" align="center">
              <Text as="p" align="center" color="gray" size="3">
                {t('app.pickDemo')}
              </Text>

            </Flex>
          </Flex>
        )}
      </main>
    </div>
  );
}

function upsert<T>(list: T[], item: T, key: (t: T) => string): T[] {
  const k = key(item);
  const idx = list.findIndex((x) => key(x) === k);
  if (idx < 0) return [item, ...list];
  const next = list.slice();
  next[idx] = item;
  return next;
}
