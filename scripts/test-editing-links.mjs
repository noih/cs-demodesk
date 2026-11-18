import assert from 'node:assert/strict';
import { test } from 'node:test';
import { recommendedReplayLinks, replayPairs, retainReplayLinks } from '../src/editingReplayLinks.ts';

const candidates = [
  ...[1,2,3].map(i => ({ id: `kill-${i}`, highlightId: 'one', kind: 'combat', tick: i * 100, replayGroup: i < 3 ? 'nearby' : 'later' })),
  { id: 'other', highlightId: 'two', kind: 'combat', tick: 100, replayGroup: 'nearby' },
];
const selected = candidates.map(c => c.id);
test('recommendations group nearby selected replays within their highlight', () => {
  assert.deepEqual(recommendedReplayLinks(candidates, selected), [{ from: 'kill-1', to: 'kill-2' }]);
  assert.deepEqual(replayPairs(candidates, selected), [{ from: 'kill-1', to: 'kill-2' }, { from: 'kill-2', to: 'kill-3' }]);
});
test('manual links survive unrelated selection changes but do not silently reconnect', () => {
  const links = replayPairs(candidates, selected);
  assert.deepEqual(retainReplayLinks(candidates, selected.filter(id => id !== 'other'), links), links);
  assert.deepEqual(retainReplayLinks(candidates, selected.filter(id => id !== 'kill-2'), links), []);
  assert.deepEqual(retainReplayLinks(candidates, selected, []), []);
});
