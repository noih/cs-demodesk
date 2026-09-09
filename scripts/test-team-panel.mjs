import assert from 'node:assert/strict';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { createServer } from 'vite';
const server = await createServer({ server: { middlewareMode: true }, appType: 'custom' });
try {
  const { default: i18n } = await import('i18next');
  const { initReactI18next } = await import('react-i18next');
  await i18n.use(initReactI18next).init({ lng: 'en', resources: { en: { translation: {} } }, initImmediate: false });
  const { TeamPanel } = await server.ssrLoadModule('/src/components/replay/TeamPanel.tsx');
  const player = { pid:1, name:'NOVA', alive:true, hp:120, armor:100, helmet:true, defuser:true, bomb:false, money:4200, weapon:'AK-47', kills:24, deaths:15, assists:4 };
  const render = p => renderToStaticMarkup(createElement(TeamPanel, {side:'CT',label:'Team A',score:8,players:[p],focus:1,onFocus:()=>{}}));
  const alive = render(player);
  assert.match(alive,/width:100%/);
  assert.match(alive,/24\/15\/4/);
  assert.match(alive,/KIT/);
  assert.match(alive,/aria-pressed="true"/);
  assert.match(render({...player,hp:17}),/health-fill low/);
  const dead = render({...player,alive:false,hp:80});
  assert.match(dead,/width:0%/);
  assert.match(dead,/24\/15\/4/);
  assert.doesNotMatch(dead,/player-kit/);
  console.log('Team panel checks passed.');
} finally { await server.close(); }
