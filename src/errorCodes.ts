import i18n from 'i18next';
import type en from './i18n/locales/en.json';

// Mirrors the application-wide Rust ErrorCode wire values.
export const ERROR_CODES = { APP_CLOSED: 'app-closed' } as const;
export type ErrorCode = typeof ERROR_CODES[keyof typeof ERROR_CODES];

export function translateError(error: string | null | undefined, code?: string): string {
  let key: keyof typeof en.errors | undefined = code === ERROR_CODES.APP_CLOSED ? code : undefined;
  if (!code && error === 'cs2.exe is already running — close the game first') key = 'cs2-already-running';
  if (!code && error === 'The old and new data directories must not contain one another.') key = 'data-directories-overlap';
  return (key && i18n.t(`errors.${key}`)) || error || '';
}
