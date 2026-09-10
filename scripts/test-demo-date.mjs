import assert from 'node:assert/strict';
import { demoDate, compareDemoDates } from '../src/demoDate.ts';

const demos = [
  { id: 'old-match', status: 'parsed', createdMs: 500, matchTimeMs: 100 },
  { id: 'new-file', status: 'new', createdMs: 300, matchTimeMs: 900 },
  { id: 'new-match', status: 'parsed', createdMs: 50, matchTimeMs: 400 },
  { id: 'no-date', status: 'parsed', createdMs: 200 },
];
assert.deepEqual(demos.toSorted(compareDemoDates).map(d => d.id), ['new-match', 'new-file', 'no-date', 'old-match']);
demos[1].status = 'parsed';
assert.equal(demos.toSorted(compareDemoDates)[0].id, 'new-file');
assert.equal(demoDate(demos[1]), 900);
console.log('Demo date ordering passed');
