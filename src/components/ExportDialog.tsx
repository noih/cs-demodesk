import { summarizeHighlights } from '../highlightWindows.ts';
import { useState, type ReactNode } from 'react';
import { Box, Button, Dialog, Flex, Grid, SegmentedControl, Select, Slider, Switch, Text } from '@radix-ui/themes';
import { useTranslation } from 'react-i18next';
import { errorText, DEFAULT_RENDER_OPTIONS, RENDER_QUALITY, type Highlight, type RenderOptions, type Status } from '../api.ts';
const RESOLUTIONS = [
  { label: '720p', width: 1280, height: 720 },
  { label: '1080p', width: 1920, height: 1080 },
  { label: '2K', width: 2560, height: 1440 },
  { label: '4K', width: 3840, height: 2160 },
];
/** HUD switches in the export dialog, in display order. */
const HUD_TOGGLES = ['hud', 'crosshair', 'radar', 'killFeed', 'chat', 'viewmodel', 'tracers', 'xray', 'trueView'] as const;

export function ExportDialog({open,onOpenChange,count,seconds,status,onSetup,onSubmit,onSubmitted,forceMerge=false,disabled=false,children,highlights,tickRate}: {
  open:boolean;onOpenChange:(open:boolean)=>void;count:number;seconds:number;status?:Status;onSetup:()=>void;
  highlights?:Highlight[];tickRate?:number;
  onSubmit:(options:RenderOptions)=>Promise<unknown>;onSubmitted:()=>void;forceMerge?:boolean;disabled?:boolean;children?:ReactNode;
}) {
  const {t}=useTranslation();
  const [opts,setOpts]=useState<RenderOptions>(DEFAULT_RENDER_OPTIONS);
  const [submitting,setSubmitting]=useState(false);
  const [error,setError]=useState<string>();
  const preview = highlights && tickRate ? summarizeHighlights(highlights, tickRate, opts.keyMomentsOnly) : undefined;
  const render=async()=>{
    if(submitting)return;setSubmitting(true);setError(undefined);
    try {await onSubmit({...opts,roundResultLabel:t('highlights.roundResult'),merge:forceMerge||opts.merge,keyMomentsOnly:!!highlights&&opts.keyMomentsOnly});onOpenChange(false);onSubmitted();}
    catch(error){setError(errorText(error));}finally{setSubmitting(false);}
  };
  return (
      <Dialog.Root open={open} onOpenChange={onOpenChange}>
        <Dialog.Content maxWidth="860px">
          <Dialog.Title>{t('highlights.dialogTitle')}</Dialog.Title>
          <Dialog.Description size="2" color="gray">
            {t('highlights.summary', { n: count, seconds: Math.round(preview?.seconds ?? seconds) })}
          </Dialog.Description>
          {error && <Text as="p" color="red" role="alert">{error}</Text>}
          {children}
          <Grid columns={{ initial: '1', sm: 'minmax(0, 1fr) minmax(0, 1fr)' }} gap="5" mt="4" align="start">
            <Flex direction="column" gap="3">
              {highlights && <Box>
                <Text as="label" size="2"><Flex gap="2" align="center">
                  <Switch size="1" checked={opts.keyMomentsOnly} onCheckedChange={keyMomentsOnly=>setOpts({...opts,keyMomentsOnly})} />
                  {t('highlights.keyMomentsOnly')}
                </Flex></Text>
                {!opts.keyMomentsOnly && opts.maxSizeMb != null && <Text as="p" role="status" size="1" color="amber" mt="1">
                  {t('highlights.fullLengthWarning')}
                </Text>}
              </Box>}
              {count > 1 && !forceMerge && (
                <Text as="label" size="2">
                  <Flex gap="2" align="center">
                    <Switch size="1" checked={opts.merge} onCheckedChange={(v) => setOpts({ ...opts, merge: v })} />
                    {t('highlights.merge')}
                  </Flex>
                </Text>
              )}
              <Box>
                <Text as="div" size="1" color="gray" mb="1">
                  {t('highlights.sizeLimit')}
                  {count > 1 && !forceMerge && (opts.merge ? t('highlights.sizeMerged') : t('highlights.sizeEach'))}
                </Text>
                <SegmentedControl.Root size="1" value={String(opts.maxSizeMb)} onValueChange={(v) => setOpts({ ...opts, maxSizeMb: v === 'null' ? null : Number(v) })} style={{ width: '100%' }}>
                  <SegmentedControl.Item value="10">10 MB</SegmentedControl.Item>
                  <SegmentedControl.Item value="20">20 MB</SegmentedControl.Item>
                  <SegmentedControl.Item value="50">50 MB</SegmentedControl.Item>
                  <SegmentedControl.Item value="null">{t('highlights.unlimited')}</SegmentedControl.Item>
                </SegmentedControl.Root>
              </Box>
              <Box>
                <Text as="div" size="1" color="gray" mb="1">
                  {t('common.resolution')}
                </Text>
                <SegmentedControl.Root
                  size="1"
                  value={String(opts.width)}
                  onValueChange={(v) => {
                    const r = RESOLUTIONS.find((x) => x.width === Number(v))!;
                    setOpts({ ...opts, width: r.width, height: r.height });
                  }}
                  style={{ width: '100%' }}
                >
                  {RESOLUTIONS.map((r) => (
                    <SegmentedControl.Item key={r.width} value={String(r.width)} title={`${r.width} × ${r.height}`}>
                      {r.label}
                    </SegmentedControl.Item>
                  ))}
                </SegmentedControl.Root>
                <Text as="p" size="1" color="gray" mt="1">{t('highlights.resolutionLimit')}</Text>
              </Box>
              <Box>
                <Text as="div" size="1" color="gray" mb="1">
                  FPS
                </Text>
                <SegmentedControl.Root size="1" value={String(opts.fps)} onValueChange={(v) => setOpts({ ...opts, fps: Number(v) })} style={{ width: '100%' }}>
                  <SegmentedControl.Item value="30">30</SegmentedControl.Item>
                  <SegmentedControl.Item value="60">60</SegmentedControl.Item>
                </SegmentedControl.Root>
              </Box>
              <Box>
                <Text as="div" size="1" color="gray" mb="1">
                  {t('highlights.encoder')}
                </Text>
                <Select.Root value={opts.codec} onValueChange={(v) => setOpts({ ...opts, codec: v, crf: RENDER_QUALITY[v] ?? DEFAULT_RENDER_OPTIONS.crf })} size="1">
                  <Select.Trigger style={{ width: '100%' }} />
                  <Select.Content>
                    <Select.Item value="libx264">H.264 · CPU（libx264）</Select.Item>
                    <Select.Item value="libx265">H.265 · CPU（libx265）</Select.Item>
                    <Select.Item value="h264_nvenc">H.264 · NVIDIA（h264_nvenc）</Select.Item>
                    <Select.Item value="hevc_nvenc">H.265 · NVIDIA（hevc_nvenc）</Select.Item>
                  </Select.Content>
                </Select.Root>
              </Box>
            </Flex>
            <Flex direction="column" gap="3">
              <Flex direction="column" gap="3">
                {HUD_TOGGLES.map((key) => (
                  <Text as="label" size="1" key={key}>
                    <Flex align="center" justify="between" gap="3">
                      <Text color="gray">{t(`highlights.toggle.${key}`)}</Text>
                      <Switch size="1" style={{ flexShrink: 0 }} checked={opts[key]} disabled={key === 'crosshair' && !opts.hud} onCheckedChange={(v) => setOpts({ ...opts, [key]: v })} />
                    </Flex>
                  </Text>
                ))}
              </Flex>
              <Box>
                <Text as="div" size="1" color="gray" mb="1">
                  {t('highlights.hudScale', { n: opts.hudScale.toFixed(2) })}
                </Text>
                <Slider min={0.5} max={0.95} step={0.05} value={[opts.hudScale]} onValueChange={([v]) => setOpts({ ...opts, hudScale: v ?? 0.85 })} />
              </Box>
            </Flex>
          </Grid>
          <Flex direction="column" mt="4" gap="4">
            <Flex direction="column" gap="2" pt="3" style={{ borderTop: '1px solid var(--app-border)' }}>
              <Text as="label" size="1">
                <Flex align="center" gap="2">
                  <Switch size="1" checked={!opts.showGame} onCheckedChange={(hideGame) => setOpts({ ...opts, showGame: !hideGame })} />
                  {t('highlights.hideGame')}
                </Flex>
              </Text>
              <Text size="1" color="gray">
                {t(opts.showGame ? 'highlights.visibleGame' : 'highlights.hiddenGame')}
              </Text>
            </Flex>
            <Flex gap="3" justify="end" align="center" wrap="wrap">
              {status && !status.ok && (
                <Button className="environment-notice" variant="soft" color="red" style={{ marginRight: 'auto' }} onClick={() => { onOpenChange(false); onSetup(); }}>
                  <i aria-hidden="true" className="bi bi-exclamation-triangle app-icon" />{status.missingRenderTools.length ? t('common.missingTools', { tools: status.missingRenderTools.join(', ') }) : t('highlights.notReady')}<i aria-hidden="true" className="bi bi-arrow-right app-icon" />
                </Button>
              )}
              <Flex gap="3" style={{ marginLeft: 'auto' }}>
                <Dialog.Close>
                  <Button variant="soft" color="gray">
                    {t('common.cancel')}
                  </Button>
                </Dialog.Close>
                <Button disabled={submitting || !status?.ok || disabled || count===0} onClick={() => void render()}>
                  {submitting ? t('highlights.submitting') : t('highlights.export')}
                </Button>
              </Flex>
            </Flex>
          </Flex>
        </Dialog.Content>
      </Dialog.Root>
  );
}
