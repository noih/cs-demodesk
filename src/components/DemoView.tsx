import { useEffect, useState } from 'react';
import { AlertDialog, Badge, Box, Button, Callout, DropdownMenu, Flex, Heading, IconButton, Spinner, Tabs, Text, Tooltip } from '@radix-ui/themes';
import { DotsHorizontalIcon, ReloadIcon } from '@radix-ui/react-icons';
import { api, errorText, mb, type DemoMeta, type ParsedDemo, type RenderJob, type Status } from '../api.ts';
import { HighlightsTab } from './HighlightsTab.tsx';
import { PlayersTab } from './PlayersTab.tsx';
import { RendersTab } from './RendersTab.tsx';
import { ChartsTab } from './ChartsTab.tsx';
import { ReplayTab } from './ReplayTab.tsx';

export function DemoView({ meta, jobs, status, onChanged, onRemoved }: { meta: DemoMeta; jobs: RenderJob[]; status?: Status; onChanged: () => Promise<void>; onRemoved: () => void }) {
  const [parsed, setParsed] = useState<ParsedDemo>();
  const [tab, setTab] = useState('highlights');
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [loadError, setLoadError] = useState<string>();

  useEffect(() => {
    let alive = true;
    api
      .demo(meta.id)
      .then((r) => alive && setParsed(r.parsed))
      .catch((e) => alive && setLoadError(errorText(e)));
    return () => {
      alive = false;
    };
  }, [meta.id, meta.status, meta.parsedAt]);

  const run = (fn: () => Promise<unknown>) => () =>
    void fn()
      .then(onChanged)
      .catch((e) => alert(errorText(e)));
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
  const activeJobs = jobs.filter((j) => j.status === 'running' || j.status === 'queued').length;

  return (
    <Flex direction="column" gap="3" style={{ height: '100%' }}>
      <Flex direction="column" gap="2">
        <Flex justify="between" align="center" gap="3">
          <Heading size="6" truncate style={{ minWidth: 0 }}>
            {parsed ? parsed.info.mapName : meta.name.replace(/\.dem$/i, '')}
          </Heading>
          <Flex gap="2" style={{ flex: 'none' }}>
            {meta.status === 'parsed' && (
              <Tooltip content="重新解析">
                <IconButton variant="soft" onClick={run(() => api.parse(meta.id))} aria-label="重新解析">
                  <ReloadIcon />
                </IconButton>
              </Tooltip>
            )}
            <DropdownMenu.Root>
              <DropdownMenu.Trigger>
                <IconButton variant="soft" aria-label="更多">
                  <DotsHorizontalIcon />
                </IconButton>
              </DropdownMenu.Trigger>
              <DropdownMenu.Content align="end">
                <DropdownMenu.Item onSelect={() => void api.reveal(meta.path)}>以檔案總管開啟</DropdownMenu.Item>
                <DropdownMenu.Item disabled={meta.status !== 'parsed'} onSelect={run(() => api.clearAnalysis(meta.id))}>
                  清除解析
                </DropdownMenu.Item>
                <DropdownMenu.Separator />
                <DropdownMenu.Item color="red" disabled={meta.status === 'parsing'} onSelect={() => setConfirmRemove(true)}>
                  刪除 demo 檔
                </DropdownMenu.Item>
              </DropdownMenu.Content>
            </DropdownMenu.Root>
          </Flex>
        </Flex>
        <Flex align="center" gap="2" wrap="wrap">
          {parsed && (
            <>
              <Badge color="blue" size="2">
                A {parsed.score.A}
              </Badge>
              <Badge color="orange" size="2">
                B {parsed.score.B}
              </Badge>
              <Text size="2" color="gray">
                {parsed.rounds.length} 回合
              </Text>
              <Text size="2" color="gray">
                ·
              </Text>
            </>
          )}
          <Text size="2" color="gray">
            {meta.name}
          </Text>
          <Text size="2" color="gray">
            · {new Date(meta.mtimeMs).toLocaleDateString()} · {mb(meta.bytes)}
          </Text>
        </Flex>
      </Flex>

      {!parsed && (
        <Flex align="center" justify="center" direction="column" gap="4" style={{ flex: 1 }}>
          {meta.status === 'parsing' ? (
            <>
              <Spinner size="3" />
              <Text color="gray">解析中…</Text>
            </>
          ) : (
            <>
              {(meta.status === 'error' || loadError) && (
                <Callout.Root color="red" size="1" style={{ maxWidth: 640 }}>
                  <Callout.Text className="selectable">{loadError ?? meta.error}</Callout.Text>
                </Callout.Root>
              )}
              <Button size="3" onClick={run(() => api.parse(meta.id))}>
                <ReloadIcon /> {meta.status === 'error' ? '重新解析' : '解析'}
              </Button>
            </>
          )}
        </Flex>
      )}

      {parsed && (
        <Tabs.Root value={tab} onValueChange={setTab} style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
          <Tabs.List>
            <Tabs.Trigger value="highlights">
              高光
              <Badge ml="2" variant="soft" color="gray">
                {parsed.highlights.length}
              </Badge>
            </Tabs.Trigger>
            <Tabs.Trigger value="players">玩家數據</Tabs.Trigger>
            <Tabs.Trigger value="charts">圖表</Tabs.Trigger>
            <Tabs.Trigger value="renders">
              影片
              {jobs.length > 0 && (
                <Badge ml="2" variant="soft" color={activeJobs ? 'amber' : 'gray'}>
                  {activeJobs ? `${activeJobs} 進行中` : jobs.length}
                </Badge>
              )}
            </Tabs.Trigger>
            <Tabs.Trigger value="2d">2D</Tabs.Trigger>
          </Tabs.List>
          <Box className="tab-body">
            <Tabs.Content value="highlights">
              <HighlightsTab meta={meta} parsed={parsed} status={status} rendering={activeJobs > 0} onRendered={() => setTab('renders')} />
            </Tabs.Content>
            <Tabs.Content value="players">
              <PlayersTab parsed={parsed} />
            </Tabs.Content>
            <Tabs.Content value="charts">
              <ChartsTab parsed={parsed} />
            </Tabs.Content>
            <Tabs.Content value="renders">
              <RendersTab jobs={jobs} parsed={parsed} onChanged={onChanged} />
            </Tabs.Content>
            <Tabs.Content value="2d" style={{ height: '100%' }}>
              <ReplayTab meta={meta} parsed={parsed} />
            </Tabs.Content>
          </Box>
        </Tabs.Root>
      )}

      <AlertDialog.Root open={confirmRemove} onOpenChange={setConfirmRemove}>
        <AlertDialog.Content maxWidth="480px">
          <AlertDialog.Title>刪除 demo 檔？</AlertDialog.Title>
          <AlertDialog.Description size="2">
            會從磁碟刪除 <span className="mono selectable">{meta.path}</span>，無法復原。已輸出的影片保留。
          </AlertDialog.Description>
          <Flex gap="3" mt="4" justify="end">
            <AlertDialog.Cancel>
              <Button variant="soft" color="gray">
                取消
              </Button>
            </AlertDialog.Cancel>
            <AlertDialog.Action>
              <Button color="red" onClick={() => void doRemove()}>
                刪除檔案
              </Button>
            </AlertDialog.Action>
          </Flex>
        </AlertDialog.Content>
      </AlertDialog.Root>
    </Flex>
  );
}
