import { useEffect, useRef, useState } from 'react';
import { Badge, Box, Button, Dialog, Flex, Select, Text } from '@radix-ui/themes';
import { useTranslation } from 'react-i18next';
import { api, errorText, type DemoMeta, type MapAssets, type ParsedDemo, type TeamKey } from '../api.ts';
import { teamStories, type StoryBeat, type TeamStory } from '../teamStories.ts';
import { Replay } from '../replay/engine.ts';
import { loadRadarImages } from '../replay/radar.ts';
import { ReplayPlayer } from './ReplayTab.tsx';
import { DialogCloseButton } from './DialogCloseButton.tsx';

export function TeamStories({ meta, parsed }: { meta: DemoMeta; parsed: ParsedDemo }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  return <>
    <Button variant="soft" onClick={() => setOpen(true)}>{t('teamStories.title')}</Button>
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Content className="app-dialog" maxWidth="min(1400px, calc(100vw - 32px))">
        <Flex justify="between" align="center" mb="3"><Dialog.Title mb="0">{t('teamStories.title')}</Dialog.Title><DialogCloseButton /></Flex>
        <Dialog.Description size="2" color="gray" mb="3">{t('teamStories.hint')}</Dialog.Description>
        <div className="app-dialog-body"><Storyboard key={`${meta.id}:${parsed.parsedAt}`} meta={meta} parsed={parsed} /></div>
      </Dialog.Content>
    </Dialog.Root>
  </>;
}

function Storyboard({ meta, parsed }: { meta: DemoMeta; parsed: ParsedDemo }) {
  const { t } = useTranslation();
  const [stories, setStories] = useState<TeamStory[]>();
  const [loaded, setLoaded] = useState<{ replay: Replay; map: MapAssets; images: ImageBitmap[] }>();
  const [error, setError] = useState<string>();
  const [mapError, setMapError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [team, setTeam] = useState<TeamKey>('A');
  const [selected, setSelected] = useState<{beat: StoryBeat; key: string}>();
  const alive = useRef(true);
  const assets = useRef<ImageBitmap[]>([]);
  const controller = useRef<AbortController | undefined>(undefined);
  useEffect(() => {
    alive.current = true;
    return () => { alive.current = false; controller.current?.abort(); assets.current.forEach(image=>image.close()); };
  }, []);
  const analyze = async () => {
    if (busy) return;
    setBusy(true); setError(undefined); setMapError(undefined);
    const abort = new AbortController(); controller.current = abort;
    try {
      const [data, kills] = await Promise.all([api.replay(meta.id), api.kills(meta.id)]);
      if (!alive.current) return;
      setStories(teamStories(parsed, data, kills));
      const replay = new Replay(data, parsed, kills);
      try {
        const map = await api.mapAssets(parsed.info.mapName);
        const images = await loadRadarImages(map.layers.map(layer=>api.fileSrc(layer.path)), abort.signal);
        if (!alive.current) { images.forEach(image=>image.close()); return; }
        assets.current.forEach(image=>image.close()); assets.current = images;
        setLoaded({replay,map,images});
      } catch (e) { if(alive.current) setMapError(errorText(e)); }
    } catch(e) { if(alive.current) setError(errorText(e)); }
    finally { if(alive.current) setBusy(false); }
  };
  const name = (id: string) => parsed.info.players.find(p=>p.steamid===id)?.name ?? id;
  return <>
    <Flex align="center" gap="3" mb="3" wrap="wrap">
      <Badge color="amber">{t('teamStories.experimental')}</Badge>
      {!stories && <Button disabled={busy} onClick={()=>void analyze()}>{busy?t('editingEffects.analyzing'):t('teamStories.analyze')}</Button>}
      {stories && <Select.Root value={team} onValueChange={value=>{setTeam(value as TeamKey);setSelected(undefined);}}><Select.Trigger aria-label={t('teamStories.team')} /><Select.Content>{(['A','B'] as const).map(key=><Select.Item key={key} value={key}>{t('teamStories.team')} {key}</Select.Item>)}</Select.Content></Select.Root>}
    </Flex>
    {error && <Text as="p" color="red" role="alert">{error}</Text>}
    {mapError && <Text as="p" size="1" color="amber">{t('teamStories.mapUnavailable')} {mapError}</Text>}
    <div className="team-story-layout">
      <Box>
        {stories?.filter(s=>s.team===team).map(story=><Box key={story.round} className="team-story-round" mb="3" p="3">
          <Flex gap="2" align="center" mb="2"><Text weight="medium">R{story.round} · {story.side==='CT'?'CT':'T'}</Text><Badge color={story.won?'green':'gray'}>{t(story.won?'teamStories.won':'teamStories.lost')}</Badge></Flex>
          {story.keyPlayer && <Text as="p" size="1" color="gray" mt="0" mb="2">{t('teamStories.keyPlayer')} · {name(story.keyPlayer)}</Text>}
          <Flex direction="column" gap="2">
            {story.beats.map((beat,index)=>{
              const key=`${team}:${story.round}:${index}`;
              const round=parsed.rounds.find(r=>r.round===story.round)!;
              return <Button key={key} variant={selected?.key===key?'solid':'soft'} size="1" disabled={!loaded} style={{height:'auto',padding:'8px',justifyContent:'start',whiteSpace:'normal',textAlign:'left'}} onClick={()=>setSelected({beat,key})}>
                +{((beat.tick-round.freezeEndTick)/parsed.info.tickRate).toFixed(1)}s · {t(`teamStories.${beat.kind}`)} · {beat.players.map(name).join(' + ')}
              </Button>;
            })}
          </Flex>
        </Box>)}
        {stories && !stories.some(s=>s.team===team) && <Text>{t('teamStories.empty')}</Text>}
      </Box>
      <Box style={{minWidth:0}}>
        {loaded && selected ? <ReplayPlayer key={selected.key} loaded={loaded} initialTick={selected.beat.start} endTick={selected.beat.end} /> : stories && <Text color="gray">{t('teamStories.select')}</Text>}
      </Box>
    </div>
  </>;
}
