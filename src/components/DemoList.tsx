import { useMemo, useRef, useState } from 'react';
import { Badge, Box, Flex, IconButton, Text, TextField, Tooltip } from '@radix-ui/themes';
import { Cross2Icon, GearIcon, MagnifyingGlassIcon, PlusIcon, ReloadIcon } from '@radix-ui/react-icons';
import { useVirtualizer } from '@tanstack/react-virtual';
import { open } from '@tauri-apps/plugin-dialog';
import { api, errorText, mb, type DemoMeta } from '../api.ts';

const STATUS_LABEL: Record<DemoMeta['status'], string> = { new: '未解析', parsing: '解析中', parsed: '已解析', error: '錯誤' };
const STATUS_COLOR: Record<DemoMeta['status'], 'gray' | 'amber' | 'green' | 'red'> = { new: 'gray', parsing: 'amber', parsed: 'green', error: 'red' };
const ROW_HEIGHT = 74;

/** "match730_003841245499151614385_1512260798_142.dem" → "match730_…_142" */
function shortName(name: string): string {
  const base = name.replace(/\.dem$/i, '');
  if (base.length <= 28) return base;
  const parts = base.split('_');
  return parts.length >= 3 ? `${parts[0]}_…_${parts[parts.length - 1]}` : `${base.slice(0, 14)}…${base.slice(-10)}`;
}

/** "2026-09-07" (local) for the date inputs */
function dayOf(ms: number): string {
  const d = new Date(ms);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

export function DemoList({
  demos,
  selectedId,
  settingsOpen,
  onSelect,
  onToggleSettings,
  onChanged,
}: {
  demos: DemoMeta[];
  selectedId?: string;
  settingsOpen: boolean;
  onSelect: (id: string) => void;
  onToggleSettings: () => void;
  onChanged: () => Promise<void>;
}) {
  const [busy, setBusy] = useState(false);
  const [query, setQuery] = useState('');
  const [from, setFrom] = useState('');
  const [to, setTo] = useState('');
  const scrollRef = useRef<HTMLDivElement>(null);

  const visible = useMemo(() => {
    // Substring match, case-insensitive; several words must all appear (any order),
    // across file name, map name and player names.
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    return demos.filter((d) => {
      if (words.length) {
        const hay = [d.name, d.mapName ?? '', ...(d.summary?.players ?? [])].join(' ').toLowerCase();
        if (!words.every((w) => hay.includes(w))) return false;
      }
      const day = dayOf(d.mtimeMs);
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
  const rescan = async () => {
    setBusy(true);
    try {
      await onChanged();
    } finally {
      setBusy(false);
    }
  };
  const filtered = visible.length !== demos.length;

  return (
    <Flex direction="column" style={{ flex: 1, minHeight: 0 }}>
      <Flex direction="column" gap="2" px="3" py="3">
        <Flex align="center" gap="2">
          <Tooltip content="加入 .dem 檔（會複製到 replays 目錄）">
            <IconButton size="2" onClick={() => void addFiles()} disabled={busy} aria-label="加入 demo">
              <PlusIcon />
            </IconButton>
          </Tooltip>
          <Tooltip content="重新掃描目錄">
            <IconButton size="2" variant="soft" onClick={() => void rescan()} disabled={busy} aria-label="重新掃描">
              <ReloadIcon />
            </IconButton>
          </Tooltip>
          <Text size="2" color="gray" style={{ flex: 1 }}>
            {filtered ? `${visible.length} / ${demos.length}` : demos.length} 個 demo
          </Text>
          <Tooltip content="設定">
            <IconButton size="2" variant={settingsOpen ? 'solid' : 'soft'} onClick={onToggleSettings} aria-label="設定">
              <GearIcon />
            </IconButton>
          </Tooltip>
        </Flex>
        <TextField.Root size="1" placeholder="搜尋檔名 / 地圖 / 玩家" value={query} onChange={(e) => setQuery(e.target.value)}>
          <TextField.Slot>
            <MagnifyingGlassIcon />
          </TextField.Slot>
          {query && (
            <TextField.Slot side="right">
              <IconButton size="1" variant="ghost" color="gray" onClick={() => setQuery('')} aria-label="清除搜尋">
                <Cross2Icon />
              </IconButton>
            </TextField.Slot>
          )}
        </TextField.Root>
        <Flex align="center" gap="1">
          <TextField.Root size="1" type="date" value={from} max={to || undefined} onChange={(e) => setFrom(e.target.value)} style={{ flex: 1, minWidth: 0 }} aria-label="起始日期" />
          <Text size="1" color="gray">
            –
          </Text>
          <TextField.Root size="1" type="date" value={to} min={from || undefined} onChange={(e) => setTo(e.target.value)} style={{ flex: 1, minWidth: 0 }} aria-label="結束日期" />
          {(from || to) && (
            <IconButton
              size="1"
              variant="ghost"
              color="gray"
              onClick={() => {
                setFrom('');
                setTo('');
              }}
              aria-label="清除日期"
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
              return (
                <Tooltip key={d.id} content={d.path} side="right">
                  <div className={`demo-item ${d.id === selectedId ? 'active' : ''}`} style={{ position: 'absolute', top: 0, left: 0, width: '100%', height: row.size, transform: `translateY(${row.start}px)` }} onClick={() => onSelect(d.id)}>
                    <Flex justify="between" align="center" gap="2">
                      <Text weight="bold" truncate>
                        {d.mapName ?? shortName(d.name)}
                      </Text>
                      <Badge size="1" color={STATUS_COLOR[d.status]} variant="soft" style={{ flex: 'none' }}>
                        {STATUS_LABEL[d.status]}
                      </Badge>
                    </Flex>
                    <Text as="div" size="1" color="gray" truncate>
                      {d.summary ? (
                        <>
                          <Text color="blue">{d.summary.scoreA}</Text> – <Text color="orange">{d.summary.scoreB}</Text> · {d.summary.highlights} 個高光
                        </>
                      ) : (
                        shortName(d.name)
                      )}
                    </Text>
                    <Text as="div" size="1" color="gray" truncate>
                      {new Date(d.mtimeMs).toLocaleDateString()} {new Date(d.mtimeMs).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })} · {mb(d.bytes)}
                    </Text>
                  </div>
                </Tooltip>
              );
            })}
          </div>
        ) : (
          <Box p="4">
            <Text size="2" color="gray">
              {demos.length === 0 ? '還沒有 demo。按 + 加入 .dem 檔，或到設定指定目錄。' : '沒有符合條件的 demo。'}
            </Text>
          </Box>
        )}
      </div>
    </Flex>
  );
}
