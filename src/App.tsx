import { useCallback, useEffect, useState } from 'react';
import { Box, Button, Callout, Flex, Spinner, Text } from '@radix-ui/themes';
import { ExclamationTriangleIcon } from '@radix-ui/react-icons';
import { api, errorText, type DemoMeta, type RenderJob, type Status } from './api.ts';
import { DemoList } from './components/DemoList.tsx';
import { DemoView } from './components/DemoView.tsx';
import { SettingsView } from './components/SettingsView.tsx';

export function App() {
  const [status, setStatus] = useState<Status>();
  const [demos, setDemos] = useState<DemoMeta[]>([]);
  const [jobs, setJobs] = useState<RenderJob[]>([]);
  const [selectedId, setSelectedId] = useState<string>();
  const [showSettings, setShowSettings] = useState(false);
  const [error, setError] = useState<string>();

  const refresh = useCallback(async () => {
    try {
      const [s, d, j] = await Promise.all([api.status(), api.demos(), api.jobs()]);
      setStatus(s);
      setDemos(d);
      setJobs(j);
      setError(undefined);
    } catch (e) {
      setError(errorText(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
    const t = setInterval(() => void refresh(), 5000);
    let unlisten: (() => void) | undefined;
    void api
      .onEvent((ev) => {
        if (ev.type === 'demo-changed') setDemos((list) => upsert(list, ev.demo, (d) => d.id));
        if (ev.type === 'job-changed') setJobs((list) => upsert(list, ev.job, (j) => j.id).sort((a, b) => b.createdAt.localeCompare(a.createdAt)));
        if (ev.type === 'setup-finished') void refresh();
      })
      .then((u) => (unlisten = u));
    return () => {
      clearInterval(t);
      unlisten?.();
    };
  }, [refresh]);

  const selected = demos.find((d) => d.id === selectedId);
  const runningJobs = jobs.filter((j) => j.status === 'running' || j.status === 'queued').length;

  return (
    <div className="layout">
      <aside className="sidebar">
        <DemoList
          demos={demos}
          selectedId={selectedId}
          settingsOpen={showSettings}
          onSelect={(id) => {
            setSelectedId(id);
            setShowSettings(false);
          }}
          onToggleSettings={() => setShowSettings((v) => !v)}
          onChanged={refresh}
        />
        {/* footer only when there is something to act on or wait for */}
        {(error || (status && !status.ok) || runningJobs > 0) && (
          <Box p="3" style={{ borderTop: '1px solid var(--gray-a4)' }}>
            {error ? (
              <Callout.Root color="red" size="1">
                <Callout.Icon>
                  <ExclamationTriangleIcon />
                </Callout.Icon>
                <Callout.Text>連不上後端：{error}</Callout.Text>
              </Callout.Root>
            ) : status && !status.ok ? (
              <Callout.Root color="red" size="1" style={{ cursor: 'pointer' }} onClick={() => setShowSettings(true)}>
                <Callout.Icon>
                  <ExclamationTriangleIcon />
                </Callout.Icon>
                <Callout.Text>渲染環境未就緒 — 開啟設定</Callout.Text>
              </Callout.Root>
            ) : (
              <Callout.Root color="amber" size="1">
                <Callout.Icon>
                  <Spinner size="1" />
                </Callout.Icon>
                <Callout.Text>渲染中 {runningJobs}</Callout.Text>
              </Callout.Root>
            )}
          </Box>
        )}
      </aside>
      <main className="main">
        {showSettings ? (
          <SettingsView onChanged={refresh} />
        ) : selected ? (
          <DemoView key={selected.id} meta={selected} jobs={jobs.filter((j) => j.demoId === selected.id)} status={status} onChanged={refresh} onRemoved={() => setSelectedId(undefined)} />
        ) : (
          <Flex align="center" justify="center" style={{ height: '100%' }}>
            <Box>
              <Text as="p" color="gray" size="3">
                選一個 demo
              </Text>
              {status && !status.ok && (
                <Button mt="3" onClick={() => setShowSettings(true)}>
                  前往設定
                </Button>
              )}
            </Box>
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
