import assert from 'node:assert/strict';
import { displayPlayerName } from '../src/playerName.ts';
for (const name of ['', '   ', '\u200b\u200d\ufeff', '\u2800\u3164\uffa0']) {
  assert.equal(displayPlayerName(name, '未命名'), '未命名');
}
for (const name of ['玩家', ' player ', '👩‍💻', 'a\u200db']) {
  assert.equal(displayPlayerName(name, '未命名'), name);
}
console.log('Player name checks passed');
