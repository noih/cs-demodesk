import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { trendSeries } from '../src/charts/trends.ts';

test('round trends accumulate, keep signed scores, and preserve missing data', () => {
  const p = { stats: [{steamid:'a',team:'A'}, {steamid:'b',team:'B'}], roundSummaries: [
    { winner:'A', players:{a:{kills:2,cash:0},b:{kills:0,cash:800}} },
    { winner:'B', players:{a:{kills:0,cash:100},b:{kills:1,cash:200}} },
    { winner:'B', players:{a:{kills:1,cash:400}} },
  ]};
  assert.deepEqual(trendSeries(p,'kills').map(s=>s.values), [[2,2,3],[0,1,null]]);
  assert.deepEqual(trendSeries(p,'difference')[0].values,[1,0,-1]);
  assert.deepEqual(trendSeries(p,'cash').map(s=>s.values),[[0,100,400],[800,200,null]]);
});

if (process.env.TIMELINE_DEMO_JSON) test('real demo round totals match the scoreboard', () => {
  const p = JSON.parse(readFileSync(process.env.TIMELINE_DEMO_JSON));
  for (const metric of ['kills','deaths','damage','flashed']) {
    for (const s of trendSeries(p,metric)) {
      const player = p.stats.find(p=>p.steamid===s.id);
      assert.equal(s.values.at(-1), metric==='flashed' ? player.activity.enemiesFlashed : player[metric], player.name+' '+metric);
    }
  }
  assert.equal(trendSeries(p,'difference')[0].values.at(-1), p.score.A-p.score.B);
  for(const s of trendSeries(p,'cash')) assert.ok(s.values.every(v=>v!=null && v>=0));
});
