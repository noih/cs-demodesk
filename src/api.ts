import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

// ---- types mirrored from the Rust side (serde camelCase) ----

export type Team = 'CT' | 'TERRORIST';

export interface PlayerInfo {
  name: string;
  steamid: string;
  teamNumber: number;
  userId?: number;
}
export interface DemoInfo {
  path: string;
  mapName: string;
  serverName: string;
  tickRate: number;
  players: PlayerInfo[];
}
export interface KillEvent {
  tick: number;
  round: number;
  attacker?: { steamid: string; name: string; team: Team; health: number; weaponName: string };
  victim: { steamid: string; name: string; team: Team; weaponName: string };
  assister?: { steamid: string; name: string; team: Team };
  weapon: string;
  headshot: boolean;
  noscope: boolean;
  penetrated: number;
  thruSmoke: boolean;
  attackerBlind: boolean;
  attackerInAir: boolean;
  assistedFlash: boolean;
  distance: number;
  hitgroup: string;
  isFreezePeriod: boolean;
}
export interface RoundInfo {
  round: number;
  startTick: number;
  freezeEndTick: number;
  endTick: number;
  officiallyEndedTick: number;
  winner?: Team;
  reason: string;
  roster: Record<string, Team>;
  bombPlantedTick?: number;
  bombDefusedTick?: number;
  bombDefuser?: string;
  bombExplodedTick?: number;
}
export interface Highlight {
  id: string;
  player: { steamid: string; name: string };
  round: number;
  startTick: number;
  endTick: number;
  anchorTick: number;
  score: number;
  tags: string[];
  title: string;
  kills: KillEvent[];
  breakdown: Record<string, number>;
}
export type TeamKey = 'A' | 'B';
export interface AimStats {
  shots: number; hits: number; headHits: number; headEligibleHits: number;
  firstShots: number; firstHits: number; sprayShots: number; sprayHits: number;
}
export interface RecoilPoint { x: number; y: number; samples: number }
export interface PlayerStats {
  steamid: string;
  name: string;
  team: TeamKey;
  kills: number;
  deaths: number;
  assists: number;
  openingKills: number;
  openingDeaths: number;
  flashAssists: number;
  roundsPlayed: number;
  roundsSurvived: number;
  kast: number;
  tradeKills: number;
  tradedDeaths: number;
  heDamage: number;
  fireDamage: number;
  opponents: Record<string, number>;
  aim: Record<string, AimStats>;
  recoil: Record<string, RecoilPoint[]>;
  activity: { shots: number; flashes: number; smokes: number; hes: number; fires: number; enemiesFlashed: number; teammatesFlashed: number; enemyBlindSeconds: number };
  clutches: { round: number; side: Team; versus: number; kills: number; outcome: 'won' | 'saved' | 'lost' }[];
  headshots: number;
  headshotPct: number;
  kd: number;
  multiKills: Record<'2k' | '3k' | '4k' | '5k', number>;
  clutchesWon: number;
  damage: number;
  utilityDamage: number;
  friendlyDamage: number;
  adr: number;
  highlights: number;
  bestScore: number;
}
export interface RoundSummary {
  players?: Record<string, { kills: number; deaths: number; damage: number; awp: number; flashed: number; cash?: number | null }>;
  round: number;
  winner?: TeamKey;
  reason: string;
  startTick: number;
  endTick: number;
  killsA: number;
  killsB: number;
  bombPlanted: boolean;
}
export interface ParsedDemo {
  info: DemoInfo;
  rounds: RoundInfo[];
  highlights: Highlight[];
  stats: PlayerStats[];
  score: { A: number; B: number };
  roundSummaries: RoundSummary[];
  parsedAt: string;
}

// ---- 2D replay (crates/demodesk-core/src/replay.rs + radar.rs) ----

export interface ReplayPlayer {
  steamid: string;
  name: string;
}
/** `p` rows: [pid, x, y, z, yaw, hp, armor, flags, weapon, money]; `g` rows: [entityId, kind, x, y, z, pid] */
export interface ReplayFrame {
  t: number;
  p: number[][];
  g?: number[][];
}
export interface ReplayEvent {
  t: number;
  k: 'shot' | 'smoke' | 'smokeEnd' | 'flash' | 'he' | 'fire' | 'fireEnd' | 'decoy' | 'decoyEnd' | 'plant' | 'defuseStart' | 'defuseAbort' | 'defuse' | 'explode' | 'bombDrop' | 'bombPickup' | 'death';
  x?: number;
  y?: number;
  z?: number;
  p?: number;
  a?: number;
  yaw?: number;
  id?: number;
  kit?: boolean;
}
export interface ReplayData {
  schemaVersion: number;
  tickRate: number;
  step: number;
  firstTick: number;
  lastTick: number;
  players: ReplayPlayer[];
  weapons: string[];
  frames: ReplayFrame[];
  events: ReplayEvent[];
}
export const FLAG = { alive: 1, helmet: 2, defuser: 4, blind: 8, bomb: 16, scoped: 32, ducking: 64, walking: 128, defusing: 256 } as const;
export const GRENADE_KINDS = ['smoke', 'flash', 'he', 'molotov', 'decoy'] as const;

export interface MapLayer {
  name: string;
  image: string;
  /** absolute path of the png */
  path: string;
  altitudeMin: number;
  altitudeMax: number;
}
export interface MapAssets {
  schemaVersion: number;
  patchVersion?: number;
  mapName: string;
  dir: string;
  posX: number;
  posY: number;
  scale: number;
  layers: MapLayer[];
}

export type DemoStatus = 'new' | 'parsing' | 'parsed' | 'error';
export interface DemoMeta {
  id: string;
  name: string;
  path: string;
  bytes: number;
  mtimeMs: number;
  status: DemoStatus;
  error?: string;
  mapName?: string;
  parsedAt?: string;
  summary?: { rounds: number; kills: number; highlights: number; scoreA: number; scoreB: number; players: string[] };
}

export type Camera = 'slot' | 'lock';
export interface RenderOptions {
  fps: number;
  width: number;
  height: number;
  codec: string;
  crf: number;
  container: string;
  camera: Camera;
  deathNoticeSeconds: number;
  /** base interface (health, ammo, score, crosshair…); the rest are separate elements */
  hud: boolean;
  /** crosshair; CS2 can only draw it while `hud` is on */
  crosshair: boolean;
  radar: boolean;
  killFeed: boolean;
  chat: boolean;
  viewmodel: boolean;
  tracers: boolean;
  hudScale: number;
  trueView: boolean;
  xray: boolean;
  voice: boolean;
  showGame: boolean;
  quitWhenDone: boolean;
  keepRawFiles: boolean;
  /** join all clips into one video; the size limit then applies to that file */
  merge: boolean;
  maxSizeMb: number | null;
  audioKbps: number;
  extraLaunchOptions: string[];
}
export const DEFAULT_RENDER_OPTIONS: RenderOptions = {
  fps: 60,
  width: 1920,
  height: 1080,
  codec: 'libx264',
  crf: 23,
  container: 'mp4',
  camera: 'slot',
  deathNoticeSeconds: 5,
  hud: true,
  crosshair: true,
  radar: true,
  killFeed: true,
  chat: false,
  viewmodel: true,
  tracers: true,
  hudScale: 0.85,
  trueView: false,
  xray: false,
  voice: false,
  showGame: false,
  quitWhenDone: true,
  keepRawFiles: false,
  merge: false,
  maxSizeMb: 20,
  audioKbps: 192,
  extraLaunchOptions: [],
};

export type JobStatus = 'queued' | 'running' | 'done' | 'error' | 'cancelled';
export interface JobOutput {
  file: string;
  bytes: number;
  highlightId?: string;
  title: string;
  isFinal: boolean;
}
export interface RenderJob {
  schemaVersion?: number;
  id: string;
  demoId: string;
  highlightIds: string[];
  options: RenderOptions;
  status: JobStatus;
  stage?: string;
  createdAt: string;
  startedAt?: string;
  finishedAt?: string;
  error?: string;
  outputs: JobOutput[];
  log: string[];
}

export interface ToolPaths {
  toolsDir: string;
  steamDir?: string;
  cs2Dir?: string;
  cs2Exe?: string;
  cs2PatchVersion?: number;
  hlaeExe?: string;
  hlaeDll?: string;
  ffmpegExe?: string;
  vrfExe?: string;
}
export interface DoctorReport {
  ok: boolean;
  problems: string[];
  paths: ToolPaths;
}
export interface Settings {
  /** UI language code, or null = follow the system language */
  language: string | null;
  cs2Dir: string | null;
  replayFolders: string[];
  scanGameReplays: boolean;
  hlaeExe: string | null;
  ffmpegExe: string | null;
  toolsDir: string | null;
}
export interface SettingsResponse {
  dataDirOverride: string | null;
  defaultDataDir: string;
  restartRequired: boolean;
  settings: Settings;
  detected: { steamDir?: string; cs2Dir?: string; replaysDir?: string };
  doctor: DoctorReport;
  setup: { running: boolean; log: string[] };
  dataDir: string;
  parsedBytes: number;
  clipsBytes: number;
  radarBytes: number;
}
export interface Status {
  ok: boolean;
  problems: string[];
  dataDir: string;
  activeRender: string | null;
  version: string;
}

export type AppEvent =
  | { type: 'demo-changed'; demo: DemoMeta }
  | { type: 'job-changed'; job: RenderJob }
  | { type: 'setup-log'; line: string }
  | { type: 'setup-finished'; ok: boolean; error: string | null };

// ---- commands ----

export const api = {
  startupError: () => invoke<string | null>('get_startup_error'),
  recoverDataDirectory: (path: string | null) => invoke<void>('recover_data_directory', { path }),
  status: () => invoke<Status>('get_status'),
  settings: () => invoke<SettingsResponse>('get_settings'),
  saveSettings: (settings: Settings, dataDirOverride: string | null) => invoke<SettingsResponse>('save_settings', { settings, dataDirOverride }),
  runSetup: (force = false) => invoke<boolean>('run_setup', { force }),
  demos: () => invoke<DemoMeta[]>('list_demos'),
  registerDemo: (path: string) => invoke<DemoMeta>('register_demo', { path }),
  parse: (id: string) => invoke<void>('parse_demo', { id }),
  demo: (id: string) => invoke<{ meta: DemoMeta; parsed?: ParsedDemo }>('get_demo', { id }),
  kills: (id: string) => invoke<KillEvent[]>('get_kills', { id }),
  replay: async (id: string): Promise<ReplayData> => {
    const f = await invoke<{ path: string; bytes: number }>('get_replay', { id });
    const worker = new Worker(new URL('./replay/load.worker.ts', import.meta.url), { type: 'module' });
    try {
      return await new Promise<ReplayData>((resolve, reject) => {
        worker.onmessage = (event: MessageEvent<{ data: ReplayData } | { error: string }>) => {
          if ('error' in event.data) reject(new Error(event.data.error));
          else resolve(event.data.data);
        };
        worker.onerror = event => reject(new Error(event.message || 'Replay worker failed'));
        worker.onmessageerror = () => reject(new Error('Replay worker response could not be read'));
        worker.postMessage(convertFileSrc(f.path));
      });
    } finally {
      worker.terminate();
    }
  },
  mapAssets: (mapName: string) => invoke<MapAssets>('get_map_assets', { mapName }),
  clearRadar: () => invoke<number>('clear_radar'),
  clearAnalysis: (id: string) => invoke<void>('clear_analysis', { id }),
  clearAllAnalysis: () => invoke<number>('clear_all_analysis'),
  clearAllClips: () => invoke<number>('clear_all_clips'),
  removeDemo: (id: string) => invoke<void>('remove_demo', { id }),
  render: (demoId: string, highlightIds: string[], options: RenderOptions) => invoke<RenderJob>('start_render', { demoId, highlightIds, options }),
  jobs: () => invoke<RenderJob[]>('list_jobs'),
  cancel: (id: string) => invoke<boolean>('cancel_job', { id }),
  deleteJob: (id: string) => invoke<void>('delete_job', { id }),
  reveal: (path: string) => invoke<void>('reveal_path', { path }),
  open: (path: string) => invoke<void>('open_path', { path }),
  openUrl: (url: string) => invoke<void>('open_url', { url }),
  onEvent: (handler: (e: AppEvent) => void): Promise<UnlistenFn> => listen<AppEvent>('demodesk://event', (ev) => handler(ev.payload)),
  fileSrc: (path: string) => convertFileSrc(path),
};

export const mb = (bytes: number): string => `${(bytes / 1024 / 1024).toFixed(1)} MB`;
/** m:ss from a number of seconds (rounded down). */
export const mmss = (seconds: number): string => {
  const s = Math.max(0, Math.floor(seconds));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
};
export const clock = (ticks: number, tickRate: number): string => mmss(ticks / tickRate);
export const errorText = (e: unknown): string => (e instanceof Error ? e.message : typeof e === 'string' ? e : JSON.stringify(e));
