// UI language. English is the source of truth (locales/en.json); the other
// files mirror its keys. Language = settings.json `language`, or the system
// language when that is null.
import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import en from './locales/en.json';
import ja from './locales/ja.json';
import zhCN from './locales/zh-CN.json';
import zhTW from './locales/zh-TW.json';

export const LANGUAGES = ['en', 'zh-TW', 'zh-CN', 'ja'] as const;
export type Language = (typeof LANGUAGES)[number];

/** Native name of each language, for the settings picker. */
export const LANGUAGE_NAMES: Record<Language, string> = { en: 'English', 'zh-TW': '繁體中文', 'zh-CN': '简体中文', ja: '日本語' };

/** Map a BCP 47 tag from the OS / WebView to one of ours; anything else is English. */
export function detectLanguage(tag: string = navigator.language): Language {
  const t = tag.toLowerCase();
  if (t.startsWith('ja')) return 'ja';
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
  resources: { en: { translation: en }, 'zh-TW': { translation: zhTW }, 'zh-CN': { translation: zhCN }, ja: { translation: ja } },
  lng: detectLanguage(),
  fallbackLng: 'en',
  interpolation: { escapeValue: false },
});

/** Date/time in the UI language rather than the OS locale. */
export const fmtDate = (ms: number) => new Date(ms).toLocaleDateString(i18n.language);
export const fmtTime = (ms: number) => new Date(ms).toLocaleTimeString(i18n.language, { hour: '2-digit', minute: '2-digit' });
export const fmtDateTime = (iso: string) => new Date(iso).toLocaleString(i18n.language);

export default i18n;
