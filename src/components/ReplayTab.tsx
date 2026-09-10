import { displayPlayerName } from '../playerName.ts';
import { useAppTheme } from '../AppTheme.tsx';
import { Spinner } from './Spinner.tsx';
import { useCallback, useEffect, useRef, useState } from 'react';
import { Box, Button, Callout, Flex, IconButton, Select, Text, Tooltip } from '@radix-ui/themes';
import { useTranslation } from 'react-i18next';
import i18n from '../i18n/index.ts';
import { api, errorText, type DemoMeta, type MapAssets, type ParsedDemo, type RoundInfo, type Team } from '../api.ts';
import { Clock, layerOf, Replay, roundAt, toImage, type TickState } from '../replay/engine.ts';
import { DEFAULT_TOGGLES, draw, layout, teamColor, type DrawToggles, type View } from '../replay/draw.ts';
import { annotationPoint, type Annotation, type DrawingTool } from '../replay/annotations.ts';
import { TeamPanel } from './replay/TeamPanel.tsx';
import { RoundTimeline } from './replay/RoundTimeline.tsx';
import { ReplayOptions } from './replay/ReplayOptions.tsx';

interface Loaded {
  replay: Replay;
  map: MapAssets;
  images: HTMLImageElement[];
}

const DRAWING_COLORS = [['pink', '#ff38b8'], ['red', '#ff4545'], ['violet', '#8759ff'], ['ocean', '#1565c0'], ['green', '#00c66b'], ['brown', '#b08060'], ['white', '#ffffff'], ['gray', '#858585']] as const;
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
          img.onerror = () => reject(new Error(i18n.t('replay.imageLoadFailed', { file: l.image })));
          img.src = api.fileSrc(l.path);
        }),
    ),
  );
  return { replay: new Replay(data, parsed, kills), map, images };
}

export function ReplayTab({ meta, parsed, onSetup }: { meta: DemoMeta; parsed: ParsedDemo; onSetup: () => void }) {
  const { t } = useTranslation();
  const [loaded, setLoaded] = useState<Loaded>();
  const [error, setError] = useState<string>();
  const [attempt, setAttempt] = useState(0);

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

  if (error) {
    return (
      <Flex direction="column" gap="3" align="start">
        {needsTools ? (
          <Button className="environment-notice" variant="soft" color="red" onClick={onSetup}>
            <i aria-hidden="true" className="bi bi-exclamation-triangle app-icon" />{t('common.missingTools', { tools: 'Source 2 Viewer' })}<i aria-hidden="true" className="bi bi-arrow-right app-icon" />
          </Button>
        ) : (
          <>
            <Callout.Root color="red" size="1"><Callout.Text className="selectable">{error}</Callout.Text></Callout.Root>
            <Button variant="soft" onClick={() => setAttempt((a) => a + 1)}>{t('common.retry')}</Button>
          </>
        )}
      </Flex>
    );
  }
  if (!loaded) {
    return (
      <Flex align="center" gap="2" p="4">
        <Spinner /> <Text color="gray">{t('replay.preparing')}</Text>
      </Flex>
    );
  }
  return <Player key={meta.id} loaded={loaded} />;
}

function Player({ loaded }: { loaded: Loaded }) {
  const { t } = useTranslation();
  const { replay, map, images } = loaded;
  const { typography } = useAppTheme();
  const labelSizeRef = useRef(typography[3]);
  useEffect(() => { labelSizeRef.current = typography[3]; }, [typography]);
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
  const annotations = useRef<Annotation[]>([]);
  const draft = useRef<Annotation | null>(null);
  const annotationRound = useRef(state.round?.round);
  const [drawing, setDrawing] = useState(false);
  const [tool, setTool] = useState<DrawingTool>('pen');
  const [color, setColor] = useState<string>(DRAWING_COLORS[0][1]);
  const [annotationCount, setAnnotationCount] = useState(0);
  const clearAnnotations = useCallback(() => {
    annotations.current = [];
    draft.current = null;
    setAnnotationCount(0);
  }, []);
  const syncAnnotationRound = useCallback((round: number | undefined) => {
    if (annotationRound.current === round) return;
    annotationRound.current = round;
    clearAnnotations();
  }, [clearAnnotations]);
  togglesRef.current = toggles;
  focusRef.current = focus;

  const seek = useCallback(
    (tick: number) => {
      clockRef.current.seek(tick);
      const next = replay.stateAt(clockRef.current.tick);
      syncAnnotationRound(next.round?.round);
      setState(next);
    },
    [replay, syncAnnotationRound],
  );
  const play = useCallback((on: boolean) => {
    const c = clockRef.current;
    if (on && c.tick >= c.last) c.seek(c.first);
    if (on) { setDrawing(false); draft.current = null; }
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
          syncAnnotationRound(s.round?.round);
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
          draw(ctx, w, h, map, images, s, togglesRef.current, viewRef.current, f, labelSizeRef.current, draft.current ? [...annotations.current, draft.current] : annotations.current);          if (now - lastUi > 100) {
            lastUi = now;
            setState(s);
          }
        }
      }
      raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(raf);
  }, [replay, map, images, syncAnnotationRound]);

  // keyboard: space play/pause, ←/→ ±5 s, PageUp/PageDown rounds, Esc unfocus
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement | null)?.tagName;
      if (tag === 'INPUT' || tag === 'SELECT' || tag === 'TEXTAREA') return;
      if (e.code === 'Space') {
        if (tag === 'BUTTON') return;
        e.preventDefault();
        play(!clockRef.current.playing);
      } else if (e.code === 'ArrowLeft') seek(clockRef.current.tick - tr * 5);
      else if (e.code === 'ArrowRight') seek(clockRef.current.tick + tr * 5);
      else if (e.code === 'PageUp') stepRound(-1);
      else if (e.code === 'PageDown') stepRound(1);
      else if (e.code === 'Escape') {
        if (drawing) { setDrawing(false); draft.current = null; }
        else toggleFocus(undefined);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [play, seek, stepRound, toggleFocus, tr, drawing]);

  // mouse: wheel zoom, drag pan (releases focus), click a disc to focus
  const drag = useRef<{ x: number; y: number } | null>(null);
  const dragged = useRef(false);
  const onWheel = (e: React.WheelEvent) => {
    if (draft.current) return;
    const box = boxRef.current!.getBoundingClientRect();
    setZoomTo(viewRef.current.zoom * (e.deltaY < 0 ? 1.15 : 1 / 1.15), e.clientX - box.left - box.width / 2, e.clientY - box.top - box.height / 2);
  };
  const onMouseMove = (e: React.MouseEvent) => {
    if (drawing || !drag.current || viewRef.current.zoom === 1) return;
    if (focusRef.current !== undefined) toggleFocus(undefined);
    viewRef.current.panX += e.clientX - drag.current.x;
    viewRef.current.panY += e.clientY - drag.current.y;
    drag.current = { x: e.clientX, y: e.clientY };
    dragged.current = true;
  };
  const onClick = (e: React.MouseEvent) => {
    if (drawing) return;
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

  const pointerPoint = (e: React.PointerEvent<HTMLCanvasElement>, layer?: number) => {
    const rect = e.currentTarget.getBoundingClientRect();
    return annotationPoint(layout(rect.width, rect.height, map.layers.length, viewRef.current), e.clientX - rect.left, e.clientY - rect.top, layer);
  };
  const startStroke = (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (!drawing || e.button !== 0 || !e.isPrimary) return;
    const hit = pointerPoint(e);
    if (!hit) return;
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    draft.current = { tool, color, layer: hit.layer, points: [hit.point] };
  };
  const moveStroke = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const stroke = draft.current;
    if (!stroke || !e.currentTarget.hasPointerCapture(e.pointerId)) return;
    const hit = pointerPoint(e, stroke.layer);
    if (!hit) return;
    if (stroke.tool !== 'pen') stroke.points = [stroke.points[0]!, hit.point];
    else stroke.points.push(hit.point);
  };
  const finishStroke = (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (!draft.current || !e.currentTarget.hasPointerCapture(e.pointerId)) return;
    moveStroke(e);
    annotations.current.push(draft.current);
    draft.current = null;
    setAnnotationCount(annotations.current.length);
    e.currentTarget.releasePointerCapture(e.pointerId);
  };
  const curRound = state.round;
  const teamLabel = (side: Team) => (curRound && replay.ctKey(curRound) === (side === 'CT' ? 'B' : 'A') ? t('common.teamB') : t('common.teamA'));

  return (
    <Flex direction="column" gap="2" style={{ height: '100%', minHeight: 0 }}>
      <Flex gap="3" align="start">
        {/* Round strip: only the bottom edge marks the winning side. */}
        <Flex gap="1" wrap="wrap" style={{ flex: 1 }}>
          {rounds.map((r) => (
            <button key={r.round} className={`round-pill ${curRound?.round === r.round ? 'active' : ''}`} aria-pressed={curRound?.round === r.round} style={{ borderBottomColor: r.winner === 'CT' ? 'var(--app-teamA)' : r.winner === 'TERRORIST' ? 'var(--app-teamB)' : 'transparent' }} onClick={() => gotoRound(r)} title={t('common.roundN', { n: r.round })}>
              {r.round}
            </button>
          ))}
        </Flex>
        {/* same width/gutter as the side panel so the button lines up with its right edge */}
        <div className={panelOpen ? 'replay-side replay-side-head' : undefined}>
          <Tooltip delayDuration={150} content={panelOpen ? t('replay.collapsePanel') : t('replay.expandPanel')}>
            <IconButton size="1" variant="soft" color="gray" onClick={() => setPanelOpen((v) => !v)} aria-label={panelOpen ? t('replay.collapsePanel') : t('replay.expandPanel')}>
              {panelOpen ? <i aria-hidden="true" className="bi bi-chevron-right app-icon"  /> : <i aria-hidden="true" className="bi bi-chevron-left app-icon"  />}
            </IconButton>
          </Tooltip>
        </div>
      </Flex>

      <Flex gap="3" style={{ flex: 1, minHeight: 0 }}>
        {/* map */}
        <Box ref={boxRef} className="replay-box" onWheel={onWheel} onMouseDown={(e) => { if (!drawing && e.target === canvasRef.current) drag.current = { x: e.clientX, y: e.clientY }; }} onMouseUp={() => (drag.current = null)} onMouseLeave={() => (drag.current = null)} onMouseMove={onMouseMove} onClick={onClick}>
          <canvas ref={canvasRef} style={{ cursor: drawing ? 'crosshair' : undefined, touchAction: drawing ? 'none' : undefined }} onPointerDown={startStroke} onPointerMove={moveStroke} onPointerUp={finishStroke} onPointerCancel={() => { draft.current = null; }} onLostPointerCapture={() => { draft.current = null; }} />
          <div className="replay-drawing-toolbar" onMouseDown={(e) => e.stopPropagation()} onClick={(e) => e.stopPropagation()} onWheel={(e) => e.stopPropagation()}>
            <IconButton color="gray" size="1" aria-label={t('replay.drawing.toggle')} title={t('replay.drawing.toggle')} variant="soft" aria-pressed={drawing} aria-expanded={drawing} onClick={() => {
              play(false);
              setDrawing(!drawing);
              draft.current = null;
              drag.current = null;
              dragged.current = false;
            }}>
              <i aria-hidden="true" className={`bi ${drawing ? 'bi-palette-fill' : 'bi-palette'} app-icon`} />
            </IconButton>
            {drawing && <div className="replay-drawing-tools">
              {([['pen', 'brush'], ['arrow', 'arrow-up-right'], ['ellipse', 'circle'], ['rectangle', 'square']] as const).map(([value, icon]) => <IconButton color="gray" key={value} size="1" variant="soft" aria-label={t(`replay.drawing.${value}`)} title={t(`replay.drawing.${value}`)} aria-pressed={tool === value} onClick={() => setTool(value)}><i aria-hidden="true" className={`bi bi-${icon} app-icon`} /></IconButton>)}
              {DRAWING_COLORS.map(([name, value]) => <button key={name} className="replay-drawing-color" style={{ backgroundColor: value }} aria-label={t(`replay.drawing.${name}`)} title={t(`replay.drawing.${name}`)} aria-pressed={color === value} onClick={() => setColor(value)} />)}
              <IconButton color="gray" size="1" variant="soft" aria-label={t('replay.drawing.undo')} title={t('replay.drawing.undo')} disabled={annotationCount === 0} onClick={() => { annotations.current.pop(); setAnnotationCount(annotations.current.length); }}><i aria-hidden="true" className="bi bi-arrow-90deg-left app-icon" /></IconButton>
              <IconButton color="gray" size="1" variant="soft" aria-label={t('replay.drawing.clear')} title={t('replay.drawing.clear')} disabled={annotationCount === 0} onClick={clearAnnotations}><i aria-hidden="true" className="bi bi-trash3 app-icon" /></IconButton>
            </div>}
          </div>
          {toggles.clock && curRound && (
            <div className="replay-hud replay-clock">
              <div className="dim">{t('common.roundN', { n: curRound.round })}</div>
              <div className={`replay-time ${state.bomb?.state === 'planted' ? 'bomb' : ''}`}>{state.clock}</div>
              {state.bomb?.state === 'planted' && <div className="replay-bomb-status"><span aria-hidden="true" />C4 · {t('charts.bombPlanted')}</div>}
            </div>
          )}
          {toggles.killFeed && state.feed.length > 0 && (
            <div className="replay-hud replay-feed">
              {state.feed.map((kv, i) => (
                <div key={`${kv.tick}-${i}`}>
                  {kv.attacker && <span style={{ color: teamColor(kv.attacker.team) }}>{displayPlayerName(kv.attacker.name)}</span>}
                  <span className="dim">
                    {' '}
                    {kv.weapon}
                    {kv.headshot && <i aria-hidden="true" className="bi bi-crosshair app-icon" />}{' '}
                  </span>
                  <span style={{ color: teamColor(kv.victim.team) }}>{displayPlayerName(kv.victim.name)}</span>
                </div>
              ))}
            </div>
          )}
          <div className="replay-hud replay-zoom">
            <IconButton size="2" variant="surface" color="gray" onClick={() => setZoomTo(viewRef.current.zoom * ZOOM_STEP)} aria-label={t('replay.zoomIn')}>
              <i aria-hidden="true" className="bi bi-plus-lg app-icon"  />
            </IconButton>
            <IconButton size="2" variant="surface" color="gray" onClick={() => setZoomTo(viewRef.current.zoom / ZOOM_STEP)} aria-label={t('replay.zoomOut')} disabled={zoom === 1}>
              <i aria-hidden="true" className="bi bi-dash app-icon"  />
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
      <Flex className="replay-transport" align="center" gap="2">
        <Tooltip delayDuration={150} content={playing ? t('replay.pauseHint') : t('replay.playHint')}>
          <IconButton onClick={() => play(!playing)} aria-label={playing ? t('replay.pause') : t('replay.play')}>
            {playing ? <i aria-hidden="true" className="bi bi-pause-fill app-icon"  /> : <i aria-hidden="true" className="bi bi-play-fill app-icon"  />}
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
        <Tooltip delayDuration={150} content={t('replay.prevRoundHint')}>
          <IconButton variant="soft" onClick={() => stepRound(-1)} aria-label={t('replay.prevRound')}>
            <i aria-hidden="true" className="bi bi-skip-start-fill app-icon"  />
          </IconButton>
        </Tooltip>
        <RoundTimeline replay={replay} round={curRound} tick={state.tick} onSeek={seek} />
        <Tooltip delayDuration={150} content={t('replay.nextRoundHint')}>
          <IconButton variant="soft" onClick={() => stepRound(1)} aria-label={t('replay.nextRound')}>
            <i aria-hidden="true" className="bi bi-skip-end-fill app-icon"  />
          </IconButton>
        </Tooltip>
      </Flex>
    </Flex>
  );
}
