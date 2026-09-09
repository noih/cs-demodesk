export type Appearance = 'light' | 'dark';
export const THEMES = {
  dark: { players: '#6aa9ff,#ffb43c,#4fd1a5,#ff6b6b,#c09fff,#55cbd1,#dfd478,#f29bc1,#aab7c4,#a3c76b', background: '#0e1012', header: '#101315', panel: '#15181b', raised: '#1c2024', border: 'rgba(255,255,255,.075)', text: '#eef1f3', muted: '#a7b0b6', subtle: '#7c868d', accent: '#ffb43c', accentText: '#241600', accentSoft: 'rgba(255,180,60,.15)', success: '#4fd1a5', danger: '#ff6b6b', teamA: '#6aa9ff', teamB: '#ff9a4d' },
  light: { players: '#1f6fd0,#946000,#087a5c,#bd3535,#7852a6,#16757a,#77700d,#a43e73,#526576,#52751f', background: '#f4f6f6', header: '#121518', panel: '#ffffff', raised: '#eceff0', border: 'rgba(12,20,24,.09)', text: '#111819', muted: '#4d585d', subtle: '#5f6a6e', accent: '#b06b00', accentText: '#ffffff', accentSoft: 'rgba(176,107,0,.13)', success: '#087a5c', danger: '#c8383d', teamA: '#1f6fd0', teamB: '#c96410' },
} satisfies Record<Appearance, Record<string, string>>;
export type AppColors = typeof THEMES[Appearance];
export function initialAppearance(saved: string | null, prefersDark: boolean): Appearance {
  return saved === 'dark' || saved === 'light' ? saved : prefersDark ? 'dark' : 'light';
}
