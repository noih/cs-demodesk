import { ConfirmDialog, Toast, useNotify } from './Notifications.tsx';
import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import { Box, Button, Callout, Card, Dialog, Flex, Grid, Heading, IconButton, Select, Switch, Text, TextField, Tooltip } from '@radix-ui/themes';
import { open } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import { api, errorText, mb, type Settings, type ToolPaths, type SettingsResponse } from '../api.ts';
import { LogView } from './LogView.tsx';
import i18n, { applyLanguage, translateProblem, detectLanguage, LANGUAGE_NAMES, LANGUAGES } from '../i18n/index.ts';

function toolsInstalled(paths: ToolPaths): boolean {
  return Boolean(paths.hlaeExe && paths.hlaeDll && paths.ffmpegExe && paths.vrfExe);
}

type PickOptions = { directory?: boolean; filters?: Array<{ name: string; extensions: string[] }> };

/** Text field + browse button; empty means "use the auto-detected value" (shown as placeholder). */
function PathField({ label, value, placeholder, hint, description, action, sourceAction, checked, source, onChange, pick }: {
  label: string; value: string; placeholder?: string; hint?: string; checked?: boolean;
  description?: string; action?: ReactNode; sourceAction?: ReactNode; source?: { repo: string; url: string; site?: 'official' | 'steam' }; onChange: (v: string) => void; pick: PickOptions;
}) {
  const { t } = useTranslation();
  const notify = useNotify();
  const browse = async () => {
    try {
      const defaultPath = await api.browseDirectory(value.trim() || placeholder || null);
      const picked = await open({ multiple: false, directory: pick.directory ?? false, filters: pick.filters, defaultPath });
      if (typeof picked === 'string') onChange(picked);
    } catch (error) {
      notify(errorText(error));
    }
  };
  return (
    <Box className={source ? 'tool-field' : undefined}>
      <Flex justify="between" align="center" mb="1" gap="2" wrap="wrap">
        <Text size="2" weight="medium">{label}</Text>
        {checked !== undefined && <Text size="1" color={checked ? 'green' : 'red'}>
          <i aria-hidden="true" className={'bi app-icon ' + (checked ? 'bi-check-circle' : 'bi-x-circle')} />
          {' '}{t(checked ? 'settings.ready' : 'settings.notReady')}
        </Text>}
      </Flex>
      {description && <Text as="div" size="1" color="gray" mb="2">{description}</Text>}
      <Flex align="center" gap="2">
        <TextField.Root size="2" aria-label={label} value={value} placeholder={placeholder ?? t('settings.notDetected')} onChange={(e) => onChange(e.target.value)} className="mono path-input" style={{ flex: 1, minWidth: 0 }} />
        {value && (
          <Tooltip delayDuration={150} content={t('settings.clearToAuto')}>
            <IconButton size="2" variant="outline" color="gray" onClick={() => onChange('')} aria-label={t('settings.clear')}>
              <i aria-hidden="true" className="bi bi-x-lg app-icon" />
            </IconButton>
          </Tooltip>
        )}
        <Button size="2" variant="outline" color="gray" onClick={() => void browse()}>
          <i aria-hidden="true" className="bi bi-folder2-open app-icon" />{t('settings.browse')}
        </Button>
        {action}
        {sourceAction ?? (source && <Tooltip delayDuration={150} content={source.site === 'steam' ? t('settings.steamStoreSource', { name: source.repo }) : source.site === 'official' ? t('settings.officialSource', { name: source.repo }) : t('settings.source', { repo: source.repo })}>
          <IconButton size="2" variant="outline" color="gray" aria-label={t('settings.sourceLabel') + ': ' + label} onClick={() => void api.openUrl(source.url)}>
            <i aria-hidden="true" className="bi bi-box-arrow-up-right app-icon" />
          </IconButton>
        </Tooltip>)}
      </Flex>
      {hint && (
        <Text as="div" size="1" color="gray" mt="1">
          {hint}
        </Text>
      )}
    </Box>
  );
}

/** Storage categories share the data folder selected above them. */
function StorageRow({ label, what, path, bytes, confirm, onClear }: { label: string; what: string; path: string; bytes: number; confirm: string; onClear: () => Promise<void> }) {
  const { t } = useTranslation();
  return (
    <>
      <Text size="2" weight="medium" style={{ minWidth: 0 }}>{label}</Text>
      <Text size="2" color="gray" align="right" style={{ fontVariantNumeric: 'tabular-nums', whiteSpace: 'nowrap' }}>{mb(bytes)}</Text>
      <IconButton size="2" variant="outline" color="gray" aria-label={t('common.openInExplorer')} onClick={() => void api.open(path)}>
        <i aria-hidden="true" className="bi bi-folder2-open app-icon"  />
      </IconButton>
      <ConfirmDialog title={t('settings.emptyTitle', { what })} description={confirm} confirmLabel={t('settings.empty')}
        onConfirm={() => void onClear()} trigger={<Button size="2" variant="outline" color="red" disabled={bytes === 0}>{t('settings.empty')}</Button>} />
    </>
  );
}

/** Where a downloaded tool comes from; shown as a link icon after its name. */
const SOURCES = {
  steam: { site: 'official' as const, repo: 'Steam', url: 'https://store.steampowered.com/about/' },
  cs2: { site: 'steam' as const, repo: 'Counter-Strike 2', url: 'https://store.steampowered.com/app/730/' },
  hlae: { repo: 'advancedfx/advancedfx', url: 'https://github.com/advancedfx/advancedfx/releases' },
  ffmpeg: { repo: 'BtbN/FFmpeg-Builds (win64 gpl)', url: 'https://github.com/BtbN/FFmpeg-Builds/releases' },
  vrf: { repo: 'ValveResourceFormat/ValveResourceFormat', url: 'https://github.com/ValveResourceFormat/ValveResourceFormat/releases' },
};

export function SettingsView({ onChanged, toolsRequest = 0, toolsTarget = 'render' }: { onChanged: () => Promise<void>; toolsRequest?: number; toolsTarget?: 'render' | 'replay' }) {
  const { t } = useTranslation();
  const [data, setData] = useState<SettingsResponse>();
  const downloadRef = useRef<HTMLButtonElement>(null);
  const guidingFocus = useRef(false);
  const [highlightTools, setHighlightTools] = useState(false);
  const loaded = data !== undefined;
  useEffect(() => {
    if (!toolsRequest || !loaded) return;
    const button = downloadRef.current;
    button?.scrollIntoView({ block: 'center', behavior: 'instant' });
    guidingFocus.current = true;
    button?.focus({ preventScroll: true });
    guidingFocus.current = false;
    setHighlightTools(true);
    const timer = setTimeout(() => setHighlightTools(false), 5000);
    return () => clearTimeout(timer);
  }, [toolsRequest, toolsTarget, loaded]);
  const [form, setForm] = useState<Settings>({ language: null, cs2Dir: null, steamDir: null, replayFolders: [], scanGameReplays: true, hlaeExe: null, ffmpegExe: null, vrfExe: null });
  const [dataDirOverride, setDataDirOverride] = useState('');
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<{ ok: boolean; text: string }>();
  const [setupLog, setSetupLog] = useState<string[]>([]);
  const [logOpen, setLogOpen] = useState(false);
  const [startingSetup, setStartingSetup] = useState(false);

  const load = useCallback(async () => {
    const r = await api.settings();
    setData(r);
    setSetupLog(r.setup.log);
    return r;
  }, []);
  useEffect(() => {
    load()
      .then((r) => { setForm(r.settings); setDataDirOverride(r.dataDirOverride ?? ''); })
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
      const r = await api.saveSettings(form, dataDirOverride || null);
      setData(r);
      setForm(r.settings);
      setDataDirOverride(r.dataDirOverride ?? '');
      applyLanguage(r.settings.language);
      // t() is still bound to the old language here; say it in the one just saved
      setMessage({ ok: true, text: t('settings.saved', { lng: r.settings.language ?? detectLanguage() }) });
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
      setMessage(!toolsInstalled(r.doctor.paths) ? { ok: false, text: t('settings.toolsIncompleteToast') }
        : r.doctor.ok ? { ok: true, text: t('settings.envReady') }
        : { ok: false, text: t('settings.envNotReady', { problems: r.doctor.problems.map(translateProblem).join('; ') }) });
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
  const runSetup = async (tool: 'hlae' | 'ffmpeg' | 'vrf') => {
    setStartingSetup(true);
    setSetupLog([]);
    setMessage(undefined);
    setLogOpen(true);
    try {
      await api.runSetup(tool, true);
      await load();
    } catch (e) {
      setMessage({ ok: false, text: errorText(e) });
    } finally {
      setStartingSetup(false);
    }
  };

  if (!data) return <Text color="gray">{t('settings.loading')}</Text>;
  const d = data.doctor;
  const toolsReady = toolsInstalled(d.paths);
  const ready = { steam: Boolean(d.paths.steamDir), cs2: Boolean(d.paths.cs2Exe), hlae: Boolean(d.paths.hlaeExe && d.paths.hlaeDll), ffmpeg: Boolean(d.paths.ffmpegExe), vrf: Boolean(d.paths.vrfExe) };
  const guidedTools = (toolsTarget === 'render' ? ['steam', 'cs2', 'hlae', 'ffmpeg'] as const : ['vrf'] as const).filter(tool => !ready[tool]);
  const toolButton = (tool: 'steam' | 'cs2' | 'hlae' | 'ffmpeg' | 'vrf', label: string) => (
    <Tooltip delayDuration={150} content={tool === 'steam' ? t('settings.officialSource', { name: label }) : tool === 'cs2' ? t('settings.steamStoreSource', { name: label }) : t('settings.downloadTool') + ': ' + label}>
      <IconButton ref={tool === guidedTools[0] ? downloadRef : undefined}
        className={highlightTools && guidedTools.includes(tool) ? 'tools-highlight' : undefined}
        size="2" variant="outline" color="gray" aria-label={t(tool === 'steam' || tool === 'cs2' ? 'settings.sourceLabel' : 'settings.downloadTool') + ': ' + label}
        onFocus={event => { if (guidingFocus.current) event.preventDefault(); }}
        disabled={startingSetup || data.setup.running} onClick={() => void (tool === 'steam' || tool === 'cs2' ? api.openUrl(SOURCES[tool].url).catch(error => setMessage({ ok: false, text: errorText(error) })) : runSetup(tool))}>
        <i aria-hidden="true" className={tool === 'steam' || tool === 'cs2' ? 'bi bi-box-arrow-up-right app-icon' : 'bi bi-download app-icon'} />
      </IconButton>
    </Tooltip>
  );

  const set = (patch: Partial<Settings>) => {
    setForm({ ...form, ...patch });
    setMessage(undefined);
  };
  const dirty = JSON.stringify(form) !== JSON.stringify(data.settings) || dataDirOverride !== (data.dataDirOverride ?? '');
  const folders = form.replayFolders.filter(Boolean);
  const gameReplays = d.paths.cs2Dir ? d.paths.cs2Dir + '/game/csgo/replays' : (form.cs2Dir ? `${form.cs2Dir}\\game\\csgo\\replays` : undefined);

  return (
    <Flex direction="column" gap="4" className="settings-page">
      <Flex justify="between" align="center" gap="3" wrap="wrap">
        <Box>
          <Heading data-text-role="title" size="6">{t('settings.title')}</Heading>
        </Box>
        <Flex gap="2" align="center">
          <Button onClick={() => void save()} disabled={saving || !dirty}>
            {saving ? t('settings.saving') : t('common.save')}
          </Button>
        </Flex>
      </Flex>

      {dirty && (
        <Callout.Root color="amber" size="1">
          <Callout.Icon>
            <i aria-hidden="true" className="bi bi-info-circle app-icon"  />
          </Callout.Icon>
          {/* in the language being picked, so the user can read it before saving */}
          <Callout.Text>{t('settings.unsaved', { lng: form.language ?? detectLanguage() })}</Callout.Text>
        </Callout.Root>
      )}

      {message && <Toast message={message.text} color={message.ok ? 'green' : 'red'} duration={message.ok ? 3000 : 0} onDismiss={() => setMessage(undefined)} />}

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
            <Heading data-text-role="subtitle" size="3" mb="3">{t('settings.demoSources')}</Heading>
            <Flex direction="column" gap="4">
              <Flex justify="between" align="center" gap="3">
                <Box style={{ minWidth: 0 }}>
                  <Text as="div" size="2" weight="medium">
                    {t('settings.scanGameReplays')}
                  </Text>
                  <Text as="div" size="1" color="gray" className="mono selectable" style={{ overflowWrap: 'anywhere', marginTop: 6 }}>
                    {gameReplays ?? t('settings.noCs2')}
                  </Text>
                </Box>
                <Switch aria-label={t('settings.scanGameReplays')} checked={form.scanGameReplays} onCheckedChange={(v) => set({ scanGameReplays: v })} />
              </Flex>
              <Box>
                <Flex justify="between" align="center" mb="1">
                  <Text size="2" weight="medium">
                    {t('settings.extraFolders')}
                  </Text>
                  <Button
                    size="2"
                    variant="outline" color="gray"
                    onClick={() =>
                      void open({ directory: true, multiple: false }).then((p) => {
                        if (typeof p === 'string' && !folders.includes(p)) set({ replayFolders: [...folders, p] });
                      })
                    }
                  >
                    <i aria-hidden="true" className="bi bi-plus-lg app-icon"  /> {t('settings.addFolder')}
                  </Button>
                </Flex>
                {folders.length === 0 ? (
                  <Text size="1" color="gray">
                    {t('settings.noExtraFolders')}
                  </Text>
                ) : (
                  <Flex direction="column" gap="1">
                    {folders.map((f) => (
                      <Flex key={f} align="center" gap="2" className="folder-row">
                        <Text size="2" className="mono selectable" style={{ flex: 1, minWidth: 0, overflowWrap: 'anywhere' }}>
                          {f}
                        </Text>
                        <IconButton size="2" variant="outline" color="gray" aria-label={t('settings.removeFolder')} onClick={() => set({ replayFolders: folders.filter((x) => x !== f) })}>
                          <i aria-hidden="true" className="bi bi-x-lg app-icon"  />
                        </IconButton>
                      </Flex>
                    ))}
                  </Flex>
                )}
              </Box>
            </Flex>
          </Card>

          <Card>
            <Heading data-text-role="subtitle" size="3" mb="3">
              {t('settings.storageSection')}
            </Heading>
            <Flex direction="column" gap="3">
              <PathField label={t('settings.dataDir')} value={dataDirOverride} placeholder={data.defaultDataDir} hint={t('settings.dataDirHint')} onChange={(value) => { setDataDirOverride(value); setMessage(undefined); }} pick={{ directory: true }} />
              {data.restartRequired && (
                <Callout.Root color="amber" size="1">
                  <Callout.Text>{t('settings.restartRequired')}</Callout.Text>
                </Callout.Root>
              )}
              <Grid columns="minmax(0, 1fr) max-content max-content max-content" gapX="3" gapY="3" align="center">
                <StorageRow label={t('settings.parsedDir')} what={t('settings.clearParsedWhat')} path={`${data.dataDir}\\parsed`} bytes={data.parsedBytes} confirm={t('settings.clearParsedConfirm')} onClear={clear(t('settings.clearParsedWhat'), api.clearAllAnalysis)} />
                <StorageRow label={t('settings.clipsDir')} what={t('settings.clearClipsWhat')} path={`${data.dataDir}\\clips`} bytes={data.clipsBytes} confirm={t('settings.clearClipsConfirm')} onClear={clear(t('settings.clearClipsWhat'), api.clearAllClips)} />
                <StorageRow label={t('settings.radarDir')} what={t('settings.clearRadarWhat')} path={`${data.dataDir}\\radar`} bytes={data.radarBytes} confirm={t('settings.clearRadarConfirm')} onClear={clear(t('settings.clearRadarWhat'), api.clearRadar)} />
              </Grid>
            </Flex>
          </Card>
        </Flex>

        {/* ---- right column: game environment & render tools ---- */}
        <Flex direction="column" gap="4">
          <Card>
            <Heading data-text-role="subtitle" size="3" mb="3">
              {t('settings.gameSection')}
            </Heading>
            <Flex direction="column" gap="3">
              <PathField label={t('settings.steamDir')} description={t('settings.steamPurpose')} source={SOURCES.steam} sourceAction={toolButton('steam', 'Steam')} checked={(form.steamDir ?? '') === (data.settings.steamDir ?? '') ? ready.steam : undefined} value={form.steamDir ?? ''} placeholder={d.paths.steamDir} onChange={value => set({ steamDir: value || null })} pick={{ directory: true }} />
              <PathField description={t('settings.cs2Purpose')} source={SOURCES.cs2} sourceAction={toolButton('cs2', 'CS2')} checked={(form.cs2Dir ?? '') === (data.settings.cs2Dir ?? '') ? Boolean(d.paths.cs2Exe) : undefined} hint={d.paths.cs2PatchVersion ? 'CS2 v' + d.paths.cs2PatchVersion : undefined} label={t('settings.cs2Dir')} value={form.cs2Dir ?? ''} placeholder={d.paths.cs2Dir} onChange={(v) => set({ cs2Dir: v || null })} pick={{ directory: true }} />
            </Flex>
          </Card>
          <Card>
            <Heading data-text-role="subtitle" size="3" mb="3">{t('settings.toolsSection')}</Heading>
            {!toolsReady && (
              <Callout.Root color="red" size="1" mb="3">
                <Callout.Icon>
                  <i aria-hidden="true" className="bi bi-exclamation-triangle app-icon"  />
                </Callout.Icon>
                <Callout.Text>
                  {t('settings.toolsIncomplete')}
                </Callout.Text>
              </Callout.Root>
            )}
            <Flex direction="column" gap="3">
              <PathField description={t('settings.hlaePurpose')} action={toolButton('hlae', 'HLAE')} source={SOURCES.hlae} checked={(form.hlaeExe ?? '') === (data.settings.hlaeExe ?? '') ? Boolean(d.paths.hlaeExe && d.paths.hlaeDll) : undefined} label="HLAE.exe" value={form.hlaeExe ?? ''} placeholder={d.paths.hlaeExe} onChange={(v) => set({ hlaeExe: v || null })} pick={{ filters: [{ name: 'HLAE', extensions: ['exe'] }] }} />
              <PathField description={t('settings.ffmpegPurpose')} action={toolButton('ffmpeg', 'FFmpeg')} source={SOURCES.ffmpeg} checked={(form.ffmpegExe ?? '') === (data.settings.ffmpegExe ?? '') ? Boolean(d.paths.ffmpegExe) : undefined} label="ffmpeg.exe" value={form.ffmpegExe ?? ''} placeholder={d.paths.ffmpegExe} onChange={(v) => set({ ffmpegExe: v || null })} pick={{ filters: [{ name: 'ffmpeg', extensions: ['exe'] }] }} />
              <PathField description={t('settings.vrfPurpose')} action={toolButton('vrf', 'Source 2 Viewer CLI')} source={SOURCES.vrf} checked={(form.vrfExe ?? '') === (data.settings.vrfExe ?? '') ? Boolean(d.paths.vrfExe) : undefined} label="Source 2 Viewer CLI" value={form.vrfExe ?? ''} placeholder={d.paths.vrfExe} onChange={(v) => set({ vrfExe: v || null })} pick={{ filters: [{ name: 'Source 2 Viewer CLI', extensions: ['exe'] }] }} />
              <Flex gap="2" wrap="wrap" align="center" justify="center">
                {data.setup.running && <Button size="2" variant="outline" color="gray" onClick={() => setLogOpen(true)}>{t('settings.downloading')}</Button>}
                <Button size="2" variant="outline" color="gray" onClick={() => void check()}>{t('settings.recheck')}</Button>
              </Flex>
            </Flex>

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
              <Button variant="outline">{t('common.close')}</Button>
            </Dialog.Close>
          </Flex>
        </Dialog.Content>
      </Dialog.Root>
    </Flex>
  );
}
