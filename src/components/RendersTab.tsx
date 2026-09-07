import { useState } from 'react';
import { AlertDialog, Badge, Box, Button, Callout, Card, DataList, Dialog, DropdownMenu, Flex, Grid, Heading, IconButton, Link, Progress, Spinner, Text } from '@radix-ui/themes';
import { DotsHorizontalIcon, ExternalLinkIcon, FileTextIcon, OpenInNewWindowIcon } from '@radix-ui/react-icons';
import { api, errorText, mb, type ParsedDemo, type RenderJob } from '../api.ts';
import { LogView } from './LogView.tsx';

const LABEL: Record<RenderJob['status'], string> = { queued: '排隊中', running: '渲染中', done: '完成', error: '失敗', cancelled: '已取消' };
const COLOR: Record<RenderJob['status'], 'gray' | 'amber' | 'green' | 'red'> = { queued: 'amber', running: 'amber', done: 'green', error: 'red', cancelled: 'gray' };
const STAGE_LABEL: Record<string, string> = { starting: '準備中', recording: '錄影中（CS2）', encoding: '編碼中' };

/** "[3/5] recording …" or "encoding 2/4" → 0..1, undefined when the stage carries no counter. */
function progressOf(job: RenderJob): number | undefined {
  const m = job.stage?.match(/(\d+)\s*\/\s*(\d+)/);
  if (!m) return undefined;
  const cur = Number(m[1]);
  const total = Number(m[2]);
  return total > 0 ? Math.min(1, cur / total) : undefined;
}

function duration(job: RenderJob): string | undefined {
  if (!job.startedAt) return undefined;
  const end = job.finishedAt ? new Date(job.finishedAt) : new Date();
  const sec = Math.max(0, Math.round((end.getTime() - new Date(job.startedAt).getTime()) / 1000));
  return sec < 60 ? `${sec} 秒` : `${Math.floor(sec / 60)} 分 ${sec % 60} 秒`;
}

function JobCard({ job, parsed, onChanged }: { job: RenderJob; parsed: ParsedDemo; onChanged: () => Promise<void> }) {
  const [logOpen, setLogOpen] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const titleOf = (id?: string) => parsed.highlights.find((h) => h.id === id)?.title;
  const act = (fn: () => Promise<unknown>) => () => void fn().then(onChanged).catch((e) => alert(errorText(e)));
  const active = job.status === 'queued' || job.status === 'running';
  const pct = progressOf(job);
  const total = job.outputs.reduce((s, o) => s + o.bytes, 0);
  const revealTarget = job.outputs[0]?.file;

  return (
    <Card>
      <Flex align="center" gap="3" wrap="wrap">
        <Badge color={COLOR[job.status]} size="2">
          {LABEL[job.status]}
        </Badge>
        <Heading size="3">{new Date(job.createdAt).toLocaleString()}</Heading>
        <Text size="2" color="gray">
          {job.highlightIds.length} 段 · {job.options.maxSizeMb ? `≤ ${job.options.maxSizeMb} MB` : '大小不限'} · {job.options.height}p{job.options.fps} · {job.options.codec}
        </Text>
        <Box style={{ flex: 1 }} />
        {active && (
          <Button size="1" variant="soft" color="red" onClick={act(() => api.cancel(job.id))}>
            取消
          </Button>
        )}
        {revealTarget && (
          <Button size="1" variant="soft" onClick={() => void api.reveal(revealTarget)}>
            <OpenInNewWindowIcon /> 開啟目錄
          </Button>
        )}
        <DropdownMenu.Root>
          <DropdownMenu.Trigger>
            <IconButton size="1" variant="soft" aria-label="更多">
              <DotsHorizontalIcon />
            </IconButton>
          </DropdownMenu.Trigger>
          <DropdownMenu.Content align="end">
            <DropdownMenu.Item onSelect={() => setLogOpen(true)}>
              <FileTextIcon /> 檢視 log（{job.log.length} 行）
            </DropdownMenu.Item>
            {!active && (
              <>
                <DropdownMenu.Separator />
                <DropdownMenu.Item color="red" onSelect={() => setConfirmDelete(true)}>
                  刪除這次輸出（含影片）
                </DropdownMenu.Item>
              </>
            )}
          </DropdownMenu.Content>
        </DropdownMenu.Root>
      </Flex>

      {active && (
        <Box mt="3">
          <Flex justify="between" align="center" mb={pct !== undefined ? '1' : '0'}>
            <Flex align="center" gap="2">
              <Spinner size="1" />
              <Text size="2" color="amber">
                {STAGE_LABEL[job.stage ?? ''] ?? job.stage ?? (job.status === 'queued' ? '等待前一個工作完成…' : '準備中…')}
              </Text>
              {job.log.length > 0 && (
                <Text size="1" color="gray" truncate style={{ maxWidth: 520 }}>
                  {job.log[job.log.length - 1]}
                </Text>
              )}
            </Flex>
            <Text size="1" color="gray">
              {duration(job)}
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
        <Grid columns={{ initial: '1', lg: 'minmax(0, 1fr) 260px' }} gap="4" mt="3" align="start">
          {/* same small preview for every file (merged video included); the player is one click away */}
          <Grid columns={{ initial: '1', sm: '2', xl: '3' }} gap="3">
            {job.outputs.map((o) => {
              const title = o.isFinal ? '合併影片' : (titleOf(o.highlightId) ?? o.title);
              return (
                <Box key={o.file}>
                  <video controls preload="metadata" src={api.fileSrc(o.file)} />
                  <Text as="div" size="2" mt="1" truncate title={title}>
                    {title}
                  </Text>
                  <Text as="div" size="1" color="gray">
                    {mb(o.bytes)} ·{' '}
                    <Link size="1" href="#" onClick={(e) => (e.preventDefault(), void api.open(o.file))}>
                      在播放器開啟 <ExternalLinkIcon style={{ verticalAlign: '-2px' }} />
                    </Link>
                  </Text>
                </Box>
              );
            })}
          </Grid>
          <DataList.Root size="1">
            <DataList.Item>
              <DataList.Label>檔案數</DataList.Label>
              <DataList.Value>{job.outputs.length}</DataList.Value>
            </DataList.Item>
            <DataList.Item>
              <DataList.Label>總大小</DataList.Label>
              <DataList.Value>{mb(total)}</DataList.Value>
            </DataList.Item>
            <DataList.Item>
              <DataList.Label>耗時</DataList.Label>
              <DataList.Value>{duration(job) ?? '—'}</DataList.Value>
            </DataList.Item>
            <DataList.Item>
              <DataList.Label>解析度</DataList.Label>
              <DataList.Value>
                {job.options.width}×{job.options.height} @ {job.options.fps}
              </DataList.Value>
            </DataList.Item>
            <DataList.Item>
              <DataList.Label>選項</DataList.Label>
              <DataList.Value>
                <Flex gap="1" wrap="wrap">
                  {job.options.trueView && <Badge size="1">TrueView</Badge>}
                  {!job.options.hud && <Badge size="1">無基本介面</Badge>}
                  {job.options.hud && !job.options.crosshair && <Badge size="1">無準星</Badge>}
                  {!job.options.radar && <Badge size="1">無雷達</Badge>}
                  {!job.options.killFeed && <Badge size="1">無擊殺訊息</Badge>}
                  {!job.options.viewmodel && <Badge size="1">無手持武器</Badge>}
                  {!job.options.tracers && <Badge size="1">無曳光彈</Badge>}
                  {job.options.chat && <Badge size="1">聊天</Badge>}
                  {job.options.xray && <Badge size="1">X-ray</Badge>}
                  {job.options.voice && <Badge size="1">語音</Badge>}
                </Flex>
              </DataList.Value>
            </DataList.Item>
            <DataList.Item>
              <DataList.Label>位置</DataList.Label>
              <DataList.Value>
                <Text className="mono selectable" style={{ wordBreak: 'break-all' }}>
                  {job.outputs[0]?.file.replace(/[\\/][^\\/]+$/, '')}
                </Text>
              </DataList.Value>
            </DataList.Item>
          </DataList.Root>
        </Grid>
      )}

      <Dialog.Root open={logOpen} onOpenChange={setLogOpen}>
        <Dialog.Content maxWidth="900px">
          <Dialog.Title>渲染 log</Dialog.Title>
          <Dialog.Description size="2" color="gray">
            {new Date(job.createdAt).toLocaleString()} · {LABEL[job.status]}
          </Dialog.Description>
          <LogView lines={job.log} empty="（尚無 log）" />
          <Flex justify="end" mt="3">
            <Dialog.Close>
              <Button variant="soft">關閉</Button>
            </Dialog.Close>
          </Flex>
        </Dialog.Content>
      </Dialog.Root>

      <AlertDialog.Root open={confirmDelete} onOpenChange={setConfirmDelete}>
        <AlertDialog.Content maxWidth="440px">
          <AlertDialog.Title>刪除這次輸出？</AlertDialog.Title>
          <AlertDialog.Description size="2">會刪掉這次工作的 {job.outputs.length} 個影片檔與紀錄；demo 與分析結果不受影響。</AlertDialog.Description>
          <Flex gap="3" mt="4" justify="end">
            <AlertDialog.Cancel>
              <Button variant="soft" color="gray">
                取消
              </Button>
            </AlertDialog.Cancel>
            <AlertDialog.Action>
              <Button color="red" onClick={act(() => api.deleteJob(job.id))}>
                刪除
              </Button>
            </AlertDialog.Action>
          </Flex>
        </AlertDialog.Content>
      </AlertDialog.Root>
    </Card>
  );
}

export function RendersTab({ jobs, parsed, onChanged }: { jobs: RenderJob[]; parsed: ParsedDemo; onChanged: () => Promise<void> }) {
  if (jobs.length === 0) {
    return (
      <Text as="p" size="2" color="gray">
        還沒有輸出的影片。
      </Text>
    );
  }
  return (
    <Flex direction="column" gap="3">
      {jobs.map((job) => (
        <JobCard key={job.id} job={job} parsed={parsed} onChanged={onChanged} />
      ))}
    </Flex>
  );
}
