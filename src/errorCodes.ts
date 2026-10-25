import i18n from 'i18next';
import type en from './i18n/locales/en.json';

// Mirrors the application-wide Rust ErrorCode wire values.
export const ERROR_CODES = {
  APP_CLOSED: 'app-closed',
  CS2_ALREADY_RUNNING: 'cs2-already-running',
  TOOLS_MISSING: 'tools-missing',
  DEMO_MISSING: 'demo-missing',
  DEMO_NOT_READY: 'demo-not-ready',
  DEMO_PATH_UNSUPPORTED: 'demo-path-unsupported',
  NO_HIGHLIGHTS: 'no-highlights',
  CAMERA_UNAVAILABLE: 'camera-unavailable',
  GAME_LAUNCH_FAILED: 'game-launch-failed',
  GAME_EXITED: 'game-exited',
  HLAE_UNAVAILABLE: 'hlae-unavailable',
  CONSOLE_UNAVAILABLE: 'console-unavailable',
  DEMO_START_FAILED: 'demo-start-failed',
  RECORDING_TIMEOUT: 'recording-timeout',
  RECORDING_FAILED: 'recording-failed',
  WINDOW_HOOK_FAILED: 'window-hook-failed',
  NO_VIDEO_RECORDED: 'no-video-recorded',
  MERGE_INCOMPLETE: 'merge-incomplete',
  ENCODER_UNAVAILABLE: 'encoder-unavailable',
  GPU_UNAVAILABLE: 'gpu-unavailable',
  ENCODING_FAILED: 'encoding-failed',
  ENCODING_STALLED: 'encoding-stalled',
  INVALID_MEDIA: 'invalid-media',
  INVALID_EXPORT_OPTIONS: 'invalid-export-options',
  SIZE_LIMIT_TOO_SMALL: 'size-limit-too-small',
  SIZE_LIMIT_UNREACHABLE: 'size-limit-unreachable',
  DISK_FULL: 'disk-full',
  PERMISSION_DENIED: 'permission-denied',
  FILE_BUSY: 'file-busy',
  FILE_UNAVAILABLE: 'file-unavailable',
  EXPORT_FAILED: 'export-failed',
  EXPORT_CANCELLED: 'export-cancelled',
} as const;
export type ErrorCode = typeof ERROR_CODES[keyof typeof ERROR_CODES];

export function translateError(error: string | null | undefined, code?: string): string {
  let key: keyof typeof en.errors | undefined = code && Object.values(ERROR_CODES).some(value => value === code)
    ? code as ErrorCode : code ? 'export-failed' : undefined;
  if (!code && error === 'no clip was recorded') key = 'no-video-recorded';
  if (!code && error === 'cs2.exe is already running — close the game first') key = 'cs2-already-running';
  if (!code && error === 'The old and new data directories must not contain one another.') key = 'data-directories-overlap';
  return (key && i18n.t(`errors.${key}`)) || error || '';
}
