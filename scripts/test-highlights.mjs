import assert from 'node:assert/strict';
import { summarizeHighlights } from '../src/highlightWindows.ts';
const h = { id:'a', player:{steamid:'p'}, round:1, startTick:0, endTick:3200, keyMoments:[[320,832],[1280,1792],[2496,3008]] };
assert.deepEqual(summarizeHighlights([h],64,true),{seconds:24,segments:3});
assert.deepEqual(summarizeHighlights([h],64,false),{seconds:50,segments:1});
for (const [gap,segments] of [[0,1],[63,1],[64,2]]) {
  const result=summarizeHighlights([{...h,keyMoments:[[1000,1256],[1256+gap,1512+gap]]}],64,true);
  assert.equal(result.segments,segments);
  assert.equal(result.seconds,gap<64?(512+gap)/64:8);
}
assert.deepEqual(summarizeHighlights([{...h,keyMoments:undefined}],64,true),{seconds:50,segments:1});
assert.deepEqual(summarizeHighlights([h,{...h,player:{steamid:'q'}}],64,true),{seconds:48,segments:6});
assert.deepEqual(summarizeHighlights([h,{...h,id:'b'}],64,true),{seconds:24,segments:3});
assert.deepEqual(summarizeHighlights([{...h,keyMoments:[[2000,2256]]}],64,true),{seconds:4,segments:1});
console.log('Highlight preview checks passed');
