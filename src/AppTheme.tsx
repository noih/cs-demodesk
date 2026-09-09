import { createContext, useContext, useEffect, useState, type ReactNode } from 'react';
import { Theme } from '@radix-ui/themes';
import { THEMES, initialAppearance, type Appearance } from './themes.ts';
// Portals create sibling Theme nodes; local inline variables do not reach them.
const themeCss = Object.entries(THEMES).map(([appearance, colors]) =>
  `.radix-themes.${appearance}{${Object.entries(colors).map(([key, value]) => `--app-${key}:${value}`).join(';')}}`,
).join('\n');
// The light theme uses a dark toolbar; portaled panels retain the page theme.
const headerCss = '.radix-themes.light .app-header{' +
  (['panel', 'raised', 'border', 'text', 'muted', 'accent', 'accentSoft'] as const)
    .map(key => `--app-${key}:${THEMES.dark[key]}`).join(';') + ';color:var(--app-text)}';
export const FONT_SIZES = { small: [20, 18, 16, 14], medium: [22, 20, 18, 16], large: [24, 22, 20, 18] } as const;
export type FontSize = keyof typeof FONT_SIZES;
const ThemeContext = createContext({ appearance: 'dark' as Appearance, colors: THEMES.dark, toggle: () => {}, fontSize: 'medium' as FontSize, setFontSize: (_size: FontSize) => {}, typography: FONT_SIZES.medium as readonly number[] });
export const useAppTheme = () => useContext(ThemeContext);
export function AppTheme({ children }: { children: ReactNode }) {
  const [appearance, setAppearance] = useState<Appearance>(() => {
    let saved: string | null = null;
    try { saved = localStorage.getItem('demodesk.appearance'); } catch { /* WebView storage may be unavailable. */ }
    return initialAppearance(saved, matchMedia('(prefers-color-scheme: dark)').matches);
  });
  const [fontSize, setFontSize] = useState<FontSize>(() => {
    try { const saved = localStorage.getItem('demodesk.fontSize'); if (saved === 'small' || saved === 'medium' || saved === 'large') return saved; } catch { /* Storage is optional. */ }
    return 'medium';
  });
  useEffect(() => { try { localStorage.setItem('demodesk.fontSize', fontSize); } catch { /* Storage is optional. */ } }, [fontSize]);
  const typography = FONT_SIZES[fontSize];
  const fontCss = '.radix-themes{' + ['title', 'subtitle', 'body', 'caption'].map((role, i) => `--app-font-${role}:${typography[i]}px`).join(';') + '}';
  const colors = THEMES[appearance];
  useEffect(() => {
    try { localStorage.setItem('demodesk.appearance', appearance); } catch { /* Theme changes still work without persistence. */ }
  }, [appearance]);
  const toggle = () => setAppearance(current => current === 'dark' ? 'light' : 'dark');
  return <ThemeContext.Provider value={{ appearance, colors, toggle, fontSize, setFontSize, typography }}><Theme panelBackground="solid" appearance={appearance} accentColor="amber" grayColor="slate" radius="small" scaling="100%"><style>{themeCss}{headerCss}{fontCss}</style><div className="app-root">{children}</div></Theme></ThemeContext.Provider>;
}
