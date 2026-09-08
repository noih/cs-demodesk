import { useCallback, useEffect, useState } from 'react';
import { AlertDialog, Badge, Box, Button, Callout, Card, DataList, Dialog, Flex, Grid, Heading, IconButton, Select, Switch, Text, TextField, Tooltip } from '@radix-ui/themes';
import { CheckCircledIcon, Cross2Icon, CrossCircledIcon, ExclamationTriangleIcon, ExternalLinkIcon, OpenInNewWindowIcon, PlusIcon } from '@radix-ui/react-icons';
import { open } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import { api, errorText, mb, type Settings, type SettingsResponse } from '../api.ts';
import { AboutDialog } from './AboutDialog.tsx';
import { LogView } from './LogView.tsx';
import i18n, { applyLanguage, LANGUAGE_NAMES, LANGUAGES } from '../i18n/index.ts';

type PickOptions = { directory?: boolean; filters?: Array<{ name: string; extensions: string[] }> };

/** Text field + browse button; empty means "use the auto-detected value" (shown as placeholder). */
function PathField({ label, value, placeholder, hint, onChange, pick }: { label: string; value: string; placeholder?: string; hint?: string; onChange: (v: string) => void; pick: PickOptions }) {
  const { t } = useTranslation();
  const browse = async () => {
    const picked = await open({ multiple: false, directory: pick.directory ?? false, filters: pick.filters });
    if (typeof picked === 'string') onChange(picked);
  };
  return (
    <Box>
      <Flex justify="between" align="center" mb="1">
        <Text size="2" weight="medium">
          {label}
        </Text>
        {value ? (
          <Badge size="1" color="amber" variant="soft">
            {t('settings.manual')}
          </Badge>
        ) : (
          <Badge size="1" color="gray" variant="soft">
            {placeholder ? t('settings.autoDetected') : t('settings.notSet')}
          </Badge>
        )}
      </Flex>
      <TextField.Root value={value} placeholder={placeholder ?? t('settings.notDetected')} onChange={(e) => onChange(e.target.value)} className="mono">
        <TextField.Slot side="right" pr="1">
          {value && (
            <Tooltip content={t('settings.clearToAuto')}>
              <IconButton size="1" variant="ghost" color="gray" onClick={() => onChange('')} aria-label={t('settings.clear')}>
                <Cross2Icon />
              </IconButton>
            </Tooltip>
          )}
          <Button size="1" variant="soft" onClick={() => void browse()}>
            {t('settings.browse')}
          </Button>
        </TextField.Slot>
      </TextField.Root>
      {hint && (
        <Text as="div" size="1" color="gray" mt="1">
          {hint}
        </Text>
      )}
    </Box>
  );
}

/** One line of the storage card: name, path, size and a confirmed "empty". */
function StorageRow({ label, what, path, bytes, confirm, onClear }: { label: string; what: string; path: string; bytes: number; confirm: string; onClear: () => Promise<void> }) {
  const { t } = useTranslation();
  // one line: label + size | path | open | clear — so the button lines up with the path
  return (
    <Flex align="center" gap="3">
      <Text size="2" weight="medium" style={{ flex: 'none', minWidth: 140 }}>
        {label} <Text color="gray">{mb(bytes)}</Text>
      </Text>
      <Text size="1" color="gray" className="mono selectable" truncate title={path} style={{ flex: 1, minWidth: 0 }}>
        {path}
      </Text>
      <IconButton size="1" variant="ghost" color="gray" aria-label={t('common.openInExplorer')} onClick={() => void api.open(path)}>
        <OpenInNewWindowIcon />
      </IconButton>
      <AlertDialog.Root>
        <AlertDialog.Trigger>
          <Button size="1" variant="soft" color="red" disabled={bytes === 0}>
            {t('settings.empty')}
          </Button>
        </AlertDialog.Trigger>
        <AlertDialog.Content maxWidth="420px">
          <AlertDialog.Title>{t('settings.emptyTitle', { what })}</AlertDialog.Title>
          <AlertDialog.Description size="2">{confirm}</AlertDialog.Description>
          <Flex gap="3" mt="4" justify="end">
            <AlertDialog.Cancel>
              <Button variant="soft" color="gray">
                {t('common.cancel')}
              </Button>
            </AlertDialog.Cancel>
            <AlertDialog.Action>
              <Button color="red" onClick={() => void onClear()}>
                {t('settings.empty')}
              </Button>
            </AlertDialog.Action>
          </Flex>
        </AlertDialog.Content>
      </AlertDialog.Root>
    </Flex>
  );
}

/** Where a downloaded tool comes from; shown as a link icon after its name. */
const SOURCES = {
  hlae: { repo: 'advancedfx/advancedfx', url: 'https://github.com/advancedfx/advancedfx/releases' },
  ffmpeg: { repo: 'BtbN/FFmpeg-Builds (win64 gpl)', url: 'https://github.com/BtbN/FFmpeg-Builds/releases' },
  vrf: { repo: 'ValveResourceFormat/ValveResourceFormat', url: 'https://github.com/ValveResourceFormat/ValveResourceFormat/releases' },
};

function CheckRow({ label, value, ok, extra, source }: { label: string; value?: string | number; ok?: boolean; extra?: string; source?: { repo: string; url: string } }) {
  const { t } = useTranslation();
  return (
    <DataList.Item>
      <DataList.Label minWidth="96px">
        <Flex align="center" gap="1">
          {ok === undefined ? null : ok ? <CheckCircledIcon color="var(--green-9)" /> : <CrossCircledIcon color="var(--red-9)" />}
          {label}
          {source && (
            <Tooltip content={t('settings.source', { repo: source.repo })}>
              <IconButton size="1" variant="ghost" color="gray" aria-label={t('settings.sourceLabel')} ml="2" onClick={() => void api.openUrl(source.url)}>
                <ExternalLinkIcon />
              </IconButton>
            </Tooltip>
          )}
        </Flex>
      </DataList.Label>
      <DataList.Value>
        <Text className="mono selectable" color={ok === false ? 'red' : undefined} style={{ wordBreak: 'break-all' }}>
          {value ?? t('settings.notFound')}
          {extra && (
            <Text color="gray" className="mono">
              {' '}
              {extra}
            </Text>
          )}
        </Text>
      </DataList.Value>
    </DataList.Item>
  );
}

export function SettingsView({ onChanged }: { onChanged: () => Promise<void> }) {
  const { t } = useTranslation();
  const [data, setData] = useState<SettingsResponse>();
  const [form, setForm] = useState<Settings>({ language: null, cs2Dir: null, replayFolders: [], scanGameReplays: true, hlaeExe: null, ffmpegExe: null, toolsDir: null });
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<{ ok: boolean; text: string }>();
  const [setupLog, setSetupLog] = useState<string[]>([]);
  const [logOpen, setLogOpen] = useState(false);

  const load = useCallback(async () => {
    const r = await api.settings();
    setData(r);
    setSetupLog(r.setup.log);
    return r;
  }, []);
  useEffect(() => {
    load()
      .then((r) => setForm(r.settings))
      .catch((e) => setMessage({ ok: false, text: errorText(e) }));
    let unlisten: (() => void) | undefined;
    void api
      .onEvent((ev) => {
        if (ev.type === 'setup-log') setSetupLog((l) => [...l, ev.line].slice(-300));
        if (ev.type === 'setup-finished') {
          void load();
          setMessage(ev.ok ? { ok: true, text: i18n.t('settings.downloadDone') } : { ok: false, text: i18n.t('settings.downloadFailed', { error: ev.error ?? i18n.t('settings.unknownError') }) });
        }
      })
      .then((u) => (unlisten = u));
    return () => unlisten?.();
  }, [load]);

  const save = async () => {
    setSaving(true);
    setMessage(undefined);
    try {
      const r = await api.saveSettings(form);
      setData(r);
      setForm(r.settings);
      applyLanguage(r.settings.language);
      setMessage({ ok: true, text: t('settings.saved') });
      await onChanged();
    } catch (e) {
      setMessage({ ok: false, text: errorText(e) });
    } finally {
      setSaving(false);
    }
  };
  /** Re-run detection with what is saved (paths typed but not saved are not checked). */
  const check = async () => {
    setMessage(undefined);
    try {
      const r = await load();
      setMessage(r.doctor.ok ? { ok: true, text: t('settings.envReady') } : { ok: false, text: t('settings.envNotReady', { problems: r.doctor.problems.join('; ') }) });
      await onChanged();
    } catch (e) {
      setMessage({ ok: false, text: errorText(e) });
    }
  };
  const clear = (what: string, fn: () => Promise<number>) => async () => {
    try {
      const freed = await fn();
      setMessage({ ok: true, text: t('settings.cleared', { what, size: mb(freed) }) });
      await load();
      await onChanged();
    } catch (e) {
      setMessage({ ok: false, text: errorText(e) });
    }
  };
  // Always a fresh download: the same button re-installs after a CS2 update breaks HLAE.
  const runSetup = async () => {
    setSetupLog([]);
    setMessage(undefined);
    setLogOpen(true);
    try {
      await api.runSetup(true);
      await load();
    } catch (e) {
      setMessage({ ok: false, text: errorText(e) });
    }
  };

  if (!data) return <Text color="gray">{t('settings.loading')}</Text>;
  const d = data.doctor;
  const set = (patch: Partial<Settings>) => setForm({ ...form, ...patch });
  const dirty = JSON.stringify(form) !== JSON.stringify(data.settings);
  const folders = form.replayFolders.filter(Boolean);
  const gameReplays = data.detected.replaysDir ?? (form.cs2Dir ? `${form.cs2Dir}\\game\\csgo\\replays` : undefined);

  return (
    <Flex direction="column" gap="4">
      <Flex justify="between" align="center" gap="3" wrap="wrap">
        <Box>
          <Heading size="6">{t('settings.title')}</Heading>
          <Text size="2" color="gray">
            {t('settings.hint')}
          </Text>
        </Box>
        <Flex gap="2" align="center">
          <AboutDialog />
          <Button onClick={() => void save()} disabled={saving || !dirty}>
            {saving ? t('settings.saving') : t('common.save')}
          </Button>
        </Flex>
      </Flex>

      {message && (
        <Callout.Root color={message.ok ? 'green' : 'red'} size="1">
          <Callout.Icon>{message.ok ? <CheckCircledIcon /> : <ExclamationTriangleIcon />}</Callout.Icon>
          <Callout.Text>{message.text}</Callout.Text>
        </Callout.Root>
      )}

      <Grid columns={{ initial: '1', md: '2' }} gap="4" align="start">
        {/* ---- left column: game & demos, output ---- */}
        <Flex direction="column" gap="4">
          <Card>
            <Flex justify="between" align="center" gap="3">
              <Text size="2" weight="medium">
                {t('settings.language')}
              </Text>
              <Select.Root value={form.language ?? 'auto'} onValueChange={(v) => set({ language: v === 'auto' ? null : v })}>
                <Select.Trigger style={{ minWidth: 160 }} />
                <Select.Content>
                  <Select.Item value="auto">{t('settings.languageAuto')}</Select.Item>
                  {LANGUAGES.map((l) => (
                    <Select.Item key={l} value={l}>
                      {LANGUAGE_NAMES[l]}
                    </Select.Item>
                  ))}
                </Select.Content>
              </Select.Root>
            </Flex>
          </Card>

          <Card>
            <Heading size="3" mb="3">
              {t('settings.gameSection')}
            </Heading>
            <Flex direction="column" gap="3">
              <PathField label={t('settings.cs2Dir')} value={form.cs2Dir ?? ''} placeholder={data.detected.cs2Dir} onChange={(v) => set({ cs2Dir: v || null })} pick={{ directory: true }} />
              <Flex justify="between" align="center" gap="3">
                <Box style={{ minWidth: 0 }}>
                  <Text as="div" size="2" weight="medium">
                    {t('settings.scanGameReplays')}
                  </Text>
                  <Text as="div" size="1" color="gray" truncate className="mono" title={gameReplays}>
                    {gameReplays ?? t('settings.noCs2')}
                  </Text>
                </Box>
                <Switch checked={form.scanGameReplays} onCheckedChange={(v) => set({ scanGameReplays: v })} />
              </Flex>
              <Box>
                <Flex justify="between" align="center" mb="1">
                  <Text size="2" weight="medium">
                    {t('settings.extraFolders')}
                  </Text>
                  <Button
                    size="1"
                    variant="soft"
                    onClick={() =>
                      void open({ directory: true, multiple: false }).then((p) => {
                        if (typeof p === 'string' && !folders.includes(p)) set({ replayFolders: [...folders, p] });
                      })
                    }
                  >
                    <PlusIcon /> {t('settings.addFolder')}
                  </Button>
                </Flex>
                {folders.length === 0 ? (
                  <Text size="1" color="gray">
                    {t('common.none')}
                  </Text>
                ) : (
                  <Flex direction="column" gap="1">
                    {folders.map((f) => (
                      <Flex key={f} align="center" gap="2" className="folder-row">
                        <Text size="2" className="mono selectable" truncate style={{ flex: 1 }} title={f}>
                          {f}
                        </Text>
                        <IconButton size="1" variant="ghost" color="gray" aria-label={t('settings.removeFolder')} onClick={() => set({ replayFolders: folders.filter((x) => x !== f) })}>
                          <Cross2Icon />
                        </IconButton>
                      </Flex>
                    ))}
                  </Flex>
                )}
              </Box>
            </Flex>
          </Card>

          <Card>
            <Heading size="3" mb="3">
              {t('settings.storageSection')}
            </Heading>
            <Flex direction="column" gap="3">
              <StorageRow label={t('settings.parsedDir')} what={t('settings.clearParsedWhat')} path={`${data.dataDir}\\parsed`} bytes={data.parsedBytes} confirm={t('settings.clearParsedConfirm')} onClear={clear(t('settings.clearParsedWhat'), api.clearAllAnalysis)} />
              <StorageRow label={t('settings.clipsDir')} what={t('settings.clearClipsWhat')} path={`${data.dataDir}\\clips`} bytes={data.clipsBytes} confirm={t('settings.clearClipsConfirm')} onClear={clear(t('settings.clearClipsWhat'), api.clearAllClips)} />
              <StorageRow label={t('settings.radarDir')} what={t('settings.clearRadarWhat')} path={`${data.dataDir}\\radar`} bytes={data.radarBytes} confirm={t('settings.clearRadarConfirm')} onClear={clear(t('settings.clearRadarWhat'), api.clearRadar)} />
            </Flex>
          </Card>
        </Flex>

        {/* ---- right column: render tools & doctor ---- */}
        <Flex direction="column" gap="4">
          <Card>
            <Flex justify="between" align="center" mb="3" gap="2">
              <Heading size="3">{t('settings.toolsSection')}</Heading>
              <Badge color={d.ok ? 'green' : 'red'} size="2">
                {d.ok ? t('settings.ready') : t('settings.notReady')}
              </Badge>
            </Flex>
            <Flex direction="column" gap="3">
              <PathField label="HLAE.exe" value={form.hlaeExe ?? ''} placeholder={d.paths.hlaeExe} onChange={(v) => set({ hlaeExe: v || null })} pick={{ filters: [{ name: 'HLAE', extensions: ['exe'] }] }} />
              <PathField label="ffmpeg.exe" value={form.ffmpegExe ?? ''} placeholder={d.paths.ffmpegExe} onChange={(v) => set({ ffmpegExe: v || null })} pick={{ filters: [{ name: 'ffmpeg', extensions: ['exe'] }] }} />
              <PathField label={t('settings.toolsDir')} value={form.toolsDir ?? ''} placeholder={d.paths.toolsDir} onChange={(v) => set({ toolsDir: v || null })} pick={{ directory: true }} />
              <Flex gap="2" wrap="wrap" align="center">
                <Button variant="soft" onClick={() => (data.setup.running ? setLogOpen(true) : void runSetup())}>
                  {data.setup.running ? t('settings.downloading') : t('settings.downloadTools')}
                </Button>
              </Flex>
            </Flex>
          </Card>

          <Card>
            <Flex justify="between" align="center" mb="3">
              <Heading size="3">{t('settings.checkSection')}</Heading>
              <Button size="1" variant="soft" color="gray" onClick={() => void check()}>
                {t('settings.recheck')}
              </Button>
            </Flex>
            <DataList.Root size="1">
              <CheckRow label="Steam" value={d.paths.steamDir} ok={Boolean(d.paths.steamDir)} />
              <CheckRow label="CS2" value={d.paths.cs2Dir} ok={Boolean(d.paths.cs2Exe)} extra={d.paths.cs2PatchVersion ? `(v${d.paths.cs2PatchVersion})` : undefined} />
              <CheckRow label="HLAE" value={d.paths.hlaeExe} ok={Boolean(d.paths.hlaeDll)} source={SOURCES.hlae} />
              <CheckRow label="FFmpeg" value={d.paths.ffmpegExe} ok={Boolean(d.paths.ffmpegExe)} source={SOURCES.ffmpeg} />
              <CheckRow label="Source 2 Viewer" value={d.paths.vrfExe} ok={Boolean(d.paths.vrfExe)} source={SOURCES.vrf} />
            </DataList.Root>
            {d.problems.length > 0 && (
              <Callout.Root color="red" size="1" mt="3">
                <Callout.Icon>
                  <ExclamationTriangleIcon />
                </Callout.Icon>
                <Callout.Text>
                  {d.problems.map((p) => (
                    <Text as="div" key={p}>
                      {p}
                    </Text>
                  ))}
                </Callout.Text>
              </Callout.Root>
            )}
          </Card>
        </Flex>
      </Grid>

      <Dialog.Root open={logOpen} onOpenChange={setLogOpen}>
        <Dialog.Content maxWidth="900px">
          <Dialog.Title>{t('settings.setupLogTitle')}</Dialog.Title>
          <Dialog.Description size="2" color="gray">
            {data.setup.running ? t('settings.setupRunning') : t('settings.setupFinished')}
          </Dialog.Description>
          <LogView lines={setupLog} empty={t('settings.setupLogEmpty')} />
          <Flex justify="end" mt="3">
            <Dialog.Close>
              <Button variant="soft">{t('common.close')}</Button>
            </Dialog.Close>
          </Flex>
        </Dialog.Content>
      </Dialog.Root>
    </Flex>
  );
}
