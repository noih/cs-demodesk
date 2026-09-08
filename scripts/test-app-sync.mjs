import assert from 'node:assert/strict';
import test from 'node:test';
import { createAppSync } from '../src/appSync.ts';

const flush = async () => { for (let i = 0; i < 12; i++) await Promise.resolve(); };
function deferred() {
  let resolve, reject;
  const promise = new Promise((ok, fail) => { resolve = ok; reject = fail; });
  return { promise, resolve, reject };
}
function fixture(t, overrides = {}) {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const calls = { listen: 0, status: 0, demos: 0, jobs: 0, stop: 0 };
  let emit;
  const backend = {
    onEvent: async handler => { calls.listen++; emit = handler; return () => calls.stop++; },
    status: async () => { calls.status++; return { ok: true }; },
    demos: async () => { calls.demos++; return []; },
    jobs: async () => { calls.jobs++; return []; },
    ...overrides,
  };
  const snapshots = [], events = [], errors = [];
  const sync = createAppSync(backend, s => snapshots.push(s), e => events.push(e), e => errors.push(e));
  t.after(() => sync.dispose());
  return { sync, calls, snapshots, events, errors, emit: e => emit(e), tick: async ms => { t.mock.timers.tick(ms); await flush(); } };
}

test('trailing debounce combines clicks and resolves all callers after the scan', async t => {
  const f = fixture(t);
  let completed = 0;
  const a = f.sync.refresh().then(() => completed++);
  await f.tick(200);
  const b = f.sync.refresh().then(() => completed++);
  await f.tick(299);
  assert.equal(f.calls.jobs, 0);
  assert.equal(completed, 0);
  await f.tick(1);
  await Promise.all([a, b]);
  assert.equal(f.calls.jobs, 1);
  assert.equal(completed, 2);
  await f.tick(10000);
  assert.equal(f.calls.jobs, 1);
});

test('clicks during a scan queue only one complete follow-up, without overlap', async t => {
  const first = deferred();
  let scans = 0;
  const f = fixture(t, { jobs: () => ++scans === 1 ? first.promise : Promise.resolve([]) });
  const a = f.sync.refresh();
  await f.tick(300);
  const b = f.sync.refresh();
  const c = f.sync.refresh();
  assert.equal(a, b);
  assert.equal(b, c);
  await f.tick(300);
  assert.equal(scans, 1);
  first.resolve([]);
  await flush();
  await Promise.all([a, b, c]);
  assert.equal(scans, 2);
  assert.equal(f.calls.status, 2);
  assert.equal(f.calls.demos, 2);
  assert.equal(f.calls.listen, 1);
});

test('follow-up waits for the trailing delay even when the active scan finishes sooner', async t => {
  const first = deferred();
  let scans = 0;
  const f = fixture(t, { jobs: () => ++scans === 1 ? first.promise : Promise.resolve([]) });
  const a = f.sync.refresh();
  await f.tick(300);
  const b = f.sync.refresh();
  first.resolve([]);
  await flush();
  await f.tick(299);
  assert.equal(scans, 1);
  await f.tick(1);
  await Promise.all([a, b]);
  assert.equal(scans, 2);
});

test('failed subscription is retried on refresh before loading, then receives live events', async t => {
  let attempts = 0, emit;
  const f = fixture(t, { onEvent: async handler => {
    attempts++;
    if (attempts === 1) throw new Error('listen failed');
    emit = handler;
    return () => {};
  } });
  const a = f.sync.refresh();
  await f.tick(300);
  assert.equal(await a, false);
  assert.equal(f.errors.length, 1);
  assert.equal(f.calls.jobs, 0);
  assert.equal(f.snapshots.length, 0);
  const b = f.sync.refresh();
  await f.tick(300);
  assert.equal(await b, true);
  assert.equal(attempts, 2);
  assert.equal(f.snapshots.length, 1);
  emit({ type: 'job-changed', job: { id: 'job-a', status: 'done' } });
  assert.equal(f.events.at(-1).job.status, 'done');
});

test('a failed API does not allow a follow-up to overlap the remaining scans', async t => {
  const slow = deferred();
  let scans = 0, checks = 0;
  const f = fixture(t, {
    status: async () => { if (++checks === 1) throw new Error('status failed'); return { ok: true }; },
    jobs: () => ++scans === 1 ? slow.promise : Promise.resolve([]),
  });
  const a = f.sync.refresh();
  await f.tick(300);
  const b = f.sync.refresh();
  await f.tick(300);
  assert.equal(scans, 1);
  slow.resolve([]);
  await flush();
  await Promise.all([a, b]);
  assert.equal(scans, 2);
  assert.equal(f.errors.length, 1);
  assert.equal(f.snapshots.length, 1);
});

test('events during a scan survive an older snapshot and include newly queued jobs', async t => {
  const slow = deferred();
  const f = fixture(t, { jobs: () => slow.promise });
  const p = f.sync.refresh();
  await f.tick(300);
  f.emit({ type: 'job-changed', job: { id: 'a', demoId: 'demo-a', status: 'done', createdAt: '2026-09-09' } });
  f.emit({ type: 'job-changed', job: { id: 'b', demoId: 'demo-b', status: 'queued', createdAt: '2026-09-10' } });
  slow.resolve([{ id: 'a', status: 'running', createdAt: '2026-09-09' }]);
  await p;
  assert.deepEqual(f.snapshots[0].jobs.map(j => [j.id, j.status]), [['b', 'queued'], ['a', 'done']]);
});

test('a later full snapshot removes deleted jobs instead of retaining prior events', async t => {
  const f = fixture(t);
  const a = f.sync.refresh();
  await f.tick(300);
  await a;
  f.emit({ type: 'job-changed', job: { id: 'a', status: 'done' } });
  const b = f.sync.refresh();
  await f.tick(300);
  await b;
  assert.deepEqual(f.snapshots.at(-1).jobs, []);
});

test('disposing during subscription setup releases a late listener and completes callers', async t => {
  const connection = deferred();
  let stopped = 0;
  const f = fixture(t, { onEvent: () => connection.promise });
  const p = f.sync.refresh();
  await f.tick(300);
  f.sync.dispose();
  await p;
  connection.resolve(() => stopped++);
  await flush();
  assert.equal(stopped, 1);
  assert.equal(f.calls.jobs, 0);
  assert.equal(f.snapshots.length, 0);
});

test('a settled refresh releases its shared promise for the next burst', async t => {
  const f = fixture(t);
  const first = f.sync.refresh();
  assert.equal(f.sync.refresh(), first);
  await f.tick(300);
  await first;
  const second = f.sync.refresh();
  assert.notEqual(second, first);
  assert.equal(f.sync.refresh(), second);
  await f.tick(300);
  await second;
  assert.equal(f.calls.jobs, 2);
});

test('a failed follow-up reports failure even if the first scan succeeded', async t => {
  const first = deferred();
  let scans = 0;
  const f = fixture(t, { jobs: () => ++scans === 1 ? first.promise : Promise.reject(new Error('scan failed')) });
  const result = f.sync.refresh();
  await f.tick(300);
  f.sync.refresh();
  await f.tick(300);
  first.resolve([]);
  assert.equal(await result, false);
  assert.equal(f.snapshots.length, 1);
  assert.equal(f.errors.length, 1);
});
