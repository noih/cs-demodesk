import { useCallback, useEffect, useRef, useState } from 'react';
import { Box, Button, Callout, Flex, IconButton, Select, Spinner, Text, Tooltip } from '@radix-ui/themes';
import { ChevronLeftIcon, ChevronRightIcon, MinusIcon, PauseIcon, PlayIcon, PlusIcon, TrackNextIcon, TrackPreviousIcon } from '@radix-ui/react-icons';
import { api, errorText, type DemoMeta, type MapAssets, type ParsedDemo, type RoundInfo, type Team } from '../api.ts';
import { Clock, layerOf, Replay, roundAt, toImage, type TickState } from '../replay/engine.ts';
import { DEFAULT_TOGGLES, draw, layout, teamColor, type DrawToggles, type View } from '../replay/draw.ts';
import { TeamPanel } from './replay/TeamPanel.tsx';
import { RoundTimeline } from './replay/RoundTimeline.tsx';
import { ReplayOptions } from './replay/ReplayOptions.tsx';

interface Loaded {
  replay: Replay;
  map: MapAssets;
  images: HTMLImageElement[];
}

const SPEEDS = ['0.5', '1', '2'];
/** zoom while following a player */
const FOCUS_ZOOM = 1.5;
const ZOOM_STEP = 1.3;

async function load(meta: DemoMeta, parsed: ParsedDemo): Promise<Loaded> {
  const [data, map, kills] = await Promise.all([api.replay(meta.id), api.mapAssets(parsed.info.mapName), api.kills(meta.id)]);
  const images = await Promise.all(
    map.layers.map(
      (l) =>
        new Promise<HTMLImageElement>((resolve, reject) => {
          const img = new Image();
          img.onload = () => resolve(img);
          img.onerror = () => reject(new Error(`無法載入 ${l.image}`));
          img.src = api.fileSrc(l.path);
        }),
    ),
  );
  return { replay: new Replay(data, parsed, kills), map, images };
}

export function ReplayTab({ meta, parsed }: { meta: DemoMeta; parsed: ParsedDemo }) {
  const [loaded, setLoaded] = useState<Loaded>();
  const [error, setError] = useState<string>();
  const [attempt, setAttempt] = useState(0);
  const [settingUp, setSettingUp] = useState(false);

  useEffect(() => {
    let alive = true;
    setError(undefined);
    load(meta, parsed)
      .then((l) => alive && setLoaded(l))
      .catch((e) => alive && setError(errorText(e)));
    return () => {
      alive = false;
    };
  }, [meta.id, meta.parsedAt, parsed, attempt]);

  const needsTools = error?.includes('Source 2 Viewer');
  const downloadTools = async () => {
    setSettingUp(true);
    try {
      await api.runSetup(false);
      // setup runs in the background; wait for it, then retry
      for (let i = 0; i < 120; i++) {
        await new Promise((r) => setTimeout(r, 2000));
        const s = await api.settings();
        if (!s.setup.running) break;
      }
      setAttempt((a) => a + 1);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setSettingUp(false);
    }
  };

  if (error) {
    return (
      <Flex direction="column" gap="3" align="start">
        <Callout.Root color="red" size="1">
          <Callout.Text className="selectable">{error}</Callout.Text>
        </Callout.Root>
        {needsTools ? (
          <Button onClick={() => void downloadTools()} disabled={settingUp}>
            {settingUp && <Spinner size="1" />} 下載工具
          </Button>
        ) : (
          <Button variant="soft" onClick={() => setAttempt((a) => a + 1)}>
            重試
          </Button>
        )}
      </Flex>
    );
  }
  if (!loaded) {
    return (
      <Flex align="center" gap="2" p="4">
        <Spinner /> <Text color="gray">準備回放資料…</Text>
      </Flex>
    );
  }
  return <Player key={meta.id} loaded={loaded} />;
}

function Player({ loaded }: { loaded: Loaded }) {
  const { replay, map, images } = loaded;
  const rounds = replay.parsed.rounds;
  const tr = replay.data.tickRate;
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const boxRef = useRef<HTMLDivElement>(null);
  // playback state lives in refs (read by the rAF loop); React state is a coarse mirror for the UI
  const clockRef = useRef(new Clock(rounds[0]?.freezeEndTick ?? replay.firstTick, tr, replay.firstTick, replay.lastTick));
  const viewRef = useRef<View>({ zoom: 1, panX: 0, panY: 0 });
  const togglesRef = useRef<DrawToggles>(DEFAULT_TOGGLES);
  const focusRef = useRef<number | undefined>(undefined);
  const [toggles, setToggles] = useState<DrawToggles>(DEFAULT_TOGGLES);
  const [playing, setPlaying] = useState(false);
  const [speed, setSpeed] = useState('1');
  const [zoom, setZoom] = useState(1);
  const [focus, setFocus] = useState<number>();
  const [panelOpen, setPanelOpen] = useState(true);
  const [state, setState] = useState<TickState>(() => replay.stateAt(clockRef.current.tick));
  togglesRef.current = toggles;
  focusRef.current = focus;

  const seek = useCallback(
    (tick: number) => {
      clockRef.current.seek(tick);
      setState(replay.stateAt(clockRef.current.tick));
    },
    [replay],
  );
  const play = useCallback((on: boolean) => {
    const c = clockRef.current;
    if (on && c.tick >= c.last) c.seek(c.first);
    c.playing = on;
    setPlaying(on);
  }, []);
  const gotoRound = useCallback(
    (r: RoundInfo | undefined) => {
      if (r) seek(r.freezeEndTick - tr * 2);
    },
    [seek, tr],
  );
  const stepRound = useCallback(
    (delta: number) => {
      const cur = roundAt(rounds, clockRef.current.tick);
      const idx = cur ? rounds.indexOf(cur) : -1;
      gotoRound(rounds[Math.min(rounds.length - 1, Math.max(0, idx + delta))]);
    },
    [rounds, gotoRound],
  );
  /** Zoom around a point (canvas px from the centre), keeping that point still. */
  const setZoomTo = useCallback((next: number, cx = 0, cy = 0) => {
    const v = viewRef.current;
    next = Math.min(8, Math.max(1, next));
    const f = next / v.zoom;
    v.panX = cx - (cx - v.panX) * f;
    v.panY = cy - (cy - v.panY) * f;
    v.zoom = next;
    if (next === 1) v.panX = v.panY = 0;
    setZoom(next);
  }, []);
  const toggleFocus = useCallback(
    (pid: number | undefined) => {
      const next = focusRef.current === pid ? undefined : pid;
      setFocus(next);
      if (next !== undefined && viewRef.current.zoom < FOCUS_ZOOM) setZoomTo(FOCUS_ZOOM);
      if (next === undefined) setZoomTo(1);
    },
    [setZoomTo],
  );

  // render loop: advance the clock, follow the focused player, draw, mirror the state ~10×/s
  useEffect(() => {
    let raf = 0;
    let last = performance.now();
    let lastUi = 0;
    const frame = (now: number) => {
      const c = clockRef.current;
      const moved = c.advance(now - last);
      last = now;
      if (moved && !c.playing) setPlaying(false);
      const canvas = canvasRef.current;
      const box = boxRef.current;
      if (canvas && box) {
        const dpr = window.devicePixelRatio || 1;
        const w = box.clientWidth;
        const h = box.clientHeight;
        if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
          canvas.width = Math.round(w * dpr);
          canvas.height = Math.round(h * dpr);
          canvas.style.width = `${w}px`;
          canvas.style.height = `${h}px`;
        }
        const ctx = canvas.getContext('2d');
        if (ctx && w > 0 && h > 0) {
          ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
          const s = replay.stateAt(c.tick);
          const f = focusRef.current;
          const target = f !== undefined ? s.players.find((p) => p.pid === f) : undefined;
          if (target?.alive) {
            const v = viewRef.current;
            const lay = layout(w, h, map.layers.length, { zoom: v.zoom, panX: 0, panY: 0 });
            const [ix, iy] = toImage(map, target.x, target.y);
            const [ox, oy] = lay.origins[layerOf(map, target.z)] ?? lay.origins[0]!;
            v.panX = w / 2 - (ox + ix * lay.k);
            v.panY = h / 2 - (oy + iy * lay.k);
          }
          draw(ctx, w, h, map, images, s, togglesRef.current, viewRef.current, f);
          if (now - lastUi > 100) {
            lastUi = now;
            setState(s);
          }
        }
      }
      raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(raf);
  }, [replay, map, images]);

  // keyboard: space play/pause, ←/→ ±5 s, PageUp/PageDown rounds, Esc unfocus
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement | null)?.tagName;
      if (tag === 'INPUT' || tag === 'SELECT' || tag === 'TEXTAREA') return;
      if (e.code === 'Space') {
        e.preventDefault();
        play(!clockRef.current.playing);
      } else if (e.code === 'ArrowLeft') seek(clockRef.current.tick - tr * 5);
      else if (e.code === 'ArrowRight') seek(clockRef.current.tick + tr * 5);
      else if (e.code === 'PageUp') stepRound(-1);
      else if (e.code === 'PageDown') stepRound(1);
      else if (e.code === 'Escape') toggleFocus(undefined);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [play, seek, stepRound, toggleFocus, tr]);

  // mouse: wheel zoom, drag pan (releases focus), click a disc to focus
  const drag = useRef<{ x: number; y: number } | null>(null);
  const dragged = useRef(false);
  const onWheel = (e: React.WheelEvent) => {
    const box = boxRef.current!.getBoundingClientRect();
    setZoomTo(viewRef.current.zoom * (e.deltaY < 0 ? 1.15 : 1 / 1.15), e.clientX - box.left - box.width / 2, e.clientY - box.top - box.height / 2);
  };
  const onMouseMove = (e: React.MouseEvent) => {
    if (!drag.current || viewRef.current.zoom === 1) return;
    if (focusRef.current !== undefined) toggleFocus(undefined);
    viewRef.current.panX += e.clientX - drag.current.x;
    viewRef.current.panY += e.clientY - drag.current.y;
    drag.current = { x: e.clientX, y: e.clientY };
    dragged.current = true;
  };
  const onClick = (e: React.MouseEvent) => {
    if (dragged.current) {
      dragged.current = false;
      return;
    }
    const box = boxRef.current!;
    const rect = box.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;
    const lay = layout(box.clientWidth, box.clientHeight, map.layers.length, viewRef.current);
    const hit = state.players.find((p) => {
      if (!p.alive) return false;
      const [ix, iy] = toImage(map, p.x, p.y);
      const [ox, oy] = lay.origins[layerOf(map, p.z)] ?? lay.origins[0]!;
      return Math.hypot(ox + ix * lay.k - mx, oy + iy * lay.k - my) < 12;
    });
    if (hit) toggleFocus(hit.pid);
  };

  const curRound = state.round;
  const teamLabel = (side: Team) => (curRound && replay.ctKey(curRound) === (side === 'CT' ? 'B' : 'A') ? 'Team B' : 'Team A');

  return (
    <Flex direction="column" gap="2" style={{ height: '100%', minHeight: 0 }}>
      <Flex gap="3" align="start">
        {/* round strip: border colour = winner side */}
        <Flex gap="1" wrap="wrap" style={{ flex: 1 }}>
          {rounds.map((r) => (
            <button key={r.round} className={`round-pill ${curRound?.round === r.round ? 'active' : ''}`} style={{ borderColor: r.winner ? teamColor(r.winner) : 'var(--gray-a6)' }} onClick={() => gotoRound(r)} title={`第 ${r.round} 回合`}>
              {r.round}
            </button>
          ))}
        </Flex>
        {/* same width/gutter as the side panel so the button lines up with its right edge */}
        <div className={panelOpen ? 'replay-side replay-side-head' : undefined}>
          <Tooltip content={panelOpen ? '收合面板' : '展開面板'}>
            <IconButton size="1" variant="soft" color="gray" onClick={() => setPanelOpen((v) => !v)} aria-label={panelOpen ? '收合面板' : '展開面板'}>
              {panelOpen ? <ChevronRightIcon /> : <ChevronLeftIcon />}
            </IconButton>
          </Tooltip>
        </div>
      </Flex>

      <Flex gap="3" style={{ flex: 1, minHeight: 0 }}>
        {/* map */}
        <Box ref={boxRef} className="replay-box" onWheel={onWheel} onMouseDown={(e) => (drag.current = { x: e.clientX, y: e.clientY })} onMouseUp={() => (drag.current = null)} onMouseLeave={() => (drag.current = null)} onMouseMove={onMouseMove} onClick={onClick}>
          <canvas ref={canvasRef} />
          {toggles.clock && curRound && (
            <div className="replay-hud replay-clock">
              <div className="dim">第 {curRound.round} 回合</div>
              <div className={`replay-time ${state.bomb?.state === 'planted' ? 'bomb' : ''}`}>{state.clock}</div>
            </div>
          )}
          {toggles.killFeed && state.feed.length > 0 && (
            <div className="replay-hud replay-feed">
              {state.feed.map((kv, i) => (
                <div key={`${kv.tick}-${i}`}>
                  {kv.attacker && <span style={{ color: teamColor(kv.attacker.team) }}>{kv.attacker.name}</span>}
                  <span className="dim">
                    {' '}
                    {kv.weapon}
                    {kv.headshot ? ' ✦' : ''}{' '}
                  </span>
                  <span style={{ color: teamColor(kv.victim.team) }}>{kv.victim.name}</span>
                </div>
              ))}
            </div>
          )}
          <div className="replay-hud replay-zoom">
            <IconButton size="2" variant="surface" color="gray" onClick={() => setZoomTo(viewRef.current.zoom * ZOOM_STEP)} aria-label="放大">
              <PlusIcon />
            </IconButton>
            <IconButton size="2" variant="surface" color="gray" onClick={() => setZoomTo(viewRef.current.zoom / ZOOM_STEP)} aria-label="縮小" disabled={zoom === 1}>
              <MinusIcon />
            </IconButton>
          </div>
          <div className="replay-hud replay-settings">
            <ReplayOptions toggles={toggles} onChange={setToggles} />
          </div>
        </Box>

        {/* teams (collapsible from the button next to the round strip) */}
        {panelOpen && (
          <Flex direction="column" gap="3" className="replay-side">
            {(['CT', 'TERRORIST'] as Team[]).map((side) => (
              <TeamPanel key={side} side={side} label={teamLabel(side)} score={side === 'CT' ? state.score.ct : state.score.t} players={state.players.filter((p) => p.team === side)} focus={focus} onFocus={toggleFocus} />
            ))}
          </Flex>
        )}
      </Flex>

      {/* transport */}
      <Flex align="center" gap="2">
        <Tooltip content={playing ? '暫停 (Space)' : '播放 (Space)'}>
          <IconButton onClick={() => play(!playing)} aria-label={playing ? '暫停' : '播放'}>
            {playing ? <PauseIcon /> : <PlayIcon />}
          </IconButton>
        </Tooltip>
        <Select.Root
          size="2"
          value={speed}
          onValueChange={(v) => {
            setSpeed(v);
            clockRef.current.speed = Number(v);
          }}
        >
          <Select.Trigger variant="soft" style={{ width: 80 }} />
          <Select.Content>
            {SPEEDS.map((s) => (
              <Select.Item key={s} value={s}>
                {s}×
              </Select.Item>
            ))}
          </Select.Content>
        </Select.Root>
        <Tooltip content="上一回合 (PageUp)">
          <IconButton variant="soft" onClick={() => stepRound(-1)} aria-label="上一回合">
            <TrackPreviousIcon />
          </IconButton>
        </Tooltip>
        <RoundTimeline replay={replay} round={curRound} tick={state.tick} onSeek={seek} />
        <Tooltip content="下一回合 (PageDown)">
          <IconButton variant="soft" onClick={() => stepRound(1)} aria-label="下一回合">
            <TrackNextIcon />
          </IconButton>
        </Tooltip>
      </Flex>
    </Flex>
  );
}
