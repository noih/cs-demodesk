// UI language. English is the source of truth (locales/en.json); the other
// files mirror its keys. Language = settings.json `language`, or the system
// language when that is null.
import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import en from './locales/en.json';
import ja from './locales/ja.json';
import ko from './locales/ko.json';
import ru from './locales/ru.json';
import zhCN from './locales/zh-CN.json';
import zhTW from './locales/zh-TW.json';

export const LANGUAGES = ['en', 'zh-TW', 'zh-CN', 'ja', 'ko', 'ru'] as const;
export type Language = (typeof LANGUAGES)[number];

/** Native name of each language, for the settings picker. */
export const LANGUAGE_NAMES: Record<Language, string> = { en: 'English', 'zh-TW': '繁體中文', 'zh-CN': '简体中文', ja: '日本語', ko: '한국어', ru: 'Русский' };

/** Map a BCP 47 tag from the OS / WebView to one of ours; anything else is English. */
export function detectLanguage(tag: string = navigator.language): Language {
  const t = tag.toLowerCase();
  if (t.startsWith('ja')) return 'ja';
  if (t.startsWith('ko')) return 'ko';
  if (t.startsWith('ru')) return 'ru';
  if (t.startsWith('zh')) return /tw|hk|mo|hant/.test(t) ? 'zh-TW' : 'zh-CN';
  return 'en';
}

export const isLanguage = (v: string | null | undefined): v is Language => LANGUAGES.includes(v as Language);

/** Apply the settings value: a language code, or null = follow the system. */
export function applyLanguage(setting: string | null | undefined) {
  void i18n.changeLanguage(isLanguage(setting) ? setting : detectLanguage());
}

// Keep <html lang> in step: CJK glyph/font selection and assistive tech follow it.
i18n.on('languageChanged', (lng) => {
  document.documentElement.lang = lng;
});

void i18n.use(initReactI18next).init({
  resources: { en: { translation: en }, 'zh-TW': { translation: zhTW }, 'zh-CN': { translation: zhCN }, ja: { translation: ja }, ko: { translation: ko }, ru: { translation: ru } },
  lng: detectLanguage(),
  fallbackLng: 'en',
  interpolation: { escapeValue: false },
});

/** Date/time in the UI language rather than the OS locale. */
export const fmtDate = (ms: number) => new Date(ms).toLocaleDateString(i18n.language);
export const fmtTime = (ms: number) => new Date(ms).toLocaleTimeString(i18n.language, { hour: '2-digit', minute: '2-digit' });
export const fmtDateTime = (iso: string) => new Date(iso).toLocaleString(i18n.language);

export default i18n;

/** Keep unknown backend diagnostics intact while translating known setup problems. */
export function translateProblem(problem: string): string {
  const keys = [
    ['rendering only runs on Windows', 'windows'], ['Steam not found', 'steam'],
    ['cs2.exe not found', 'cs2'], ['HLAE not installed', 'hlae'],
    ['x64/AfxHookSource2.dll missing', 'hook'], ['ffmpeg.exe not found', 'ffmpeg'],
  ] as const;
  const key = keys.find(([prefix]) => problem.startsWith(prefix))?.[1];
  return key ? i18n.t(`diagnostics.${key}`) : problem;
}

/** Translate complete known stages; preserve unfamiliar diagnostics verbatim. */
export function translateRenderStage(stage: string): string {
  const match = /^(starting|recording|encoding)(?: (\d+\/\d+))?(?:: (seeking|setup|muxing|merging|fitting))?$/.exec(stage);
  if (!match) return stage;
  const [, phase, count, detail] = match;
  const isKey = (key: string): key is keyof typeof en.renders.stage => Object.hasOwn(en.renders.stage, key);
  if (!phase || !isKey(phase) || (detail && !isKey(detail))) return stage;
  return `${i18n.t(`renders.stage.${phase}`)}${count ? ` ${count}` : ''}${detail && isKey(detail) ? ` · ${i18n.t(`renders.stage.${detail}`)}` : ''}`;
}
