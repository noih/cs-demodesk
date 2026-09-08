import type { api, AppEvent, DemoMeta, RenderJob, Status } from './api.ts';

const REFRESH_DEBOUNCE_MS = 300;
interface Snapshot { status: Status; demos: DemoMeta[]; jobs: RenderJob[] }

export function createAppSync(
  backend: Pick<typeof api, 'onEvent' | 'status' | 'demos' | 'jobs'>,
  onSnapshot: (snapshot: Snapshot) => void,
  onEvent: (event: AppEvent) => void,
  onError: (error: unknown) => void,
) {
  let disposed = false;
  let running = false;
  let ready = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let unlisten: (() => void) | undefined;
  let completion: Promise<boolean> | undefined;
  let resolveCompletion: ((success: boolean) => void) | undefined;
  let updates: { demos: Map<string, DemoMeta>; jobs: Map<string, RenderJob> } | undefined;

  let succeeded = false;

  function settle() {
    resolveCompletion?.(succeeded && !disposed);
    completion = undefined;
    resolveCompletion = undefined;
  }

  function refresh(): Promise<boolean> {
    if (disposed) return Promise.resolve(false);
    ready = false;
    clearTimeout(timer);
    timer = setTimeout(() => {
      timer = undefined;
      ready = true;
      void run();
    }, REFRESH_DEBOUNCE_MS);
    completion ??= new Promise<boolean>((resolve) => { resolveCompletion = resolve; });
    return completion;
  }

  async function run() {
    if (disposed || running || !ready) return;
    ready = false;
    running = true;
    succeeded = false;
    try {
      if (!unlisten) {
        const stop = await backend.onEvent((event) => {
          if (disposed) return;
          if (event.type === 'demo-changed') updates?.demos.set(event.demo.id, event.demo);
          if (event.type === 'job-changed') updates?.jobs.set(event.job.id, event.job);
          onEvent(event);
          if (event.type === 'setup-finished') void refresh();
        });
        if (disposed) { stop(); return; }
        unlisten = stop;
      }
      const pending = { demos: new Map<string, DemoMeta>(), jobs: new Map<string, RenderJob>() };
      updates = pending;
      // Wait for every scan, including when one fails, before starting another pass.
      const results = await Promise.allSettled([backend.status(), backend.demos(), backend.jobs()]);
      if (disposed) return;
      const [status, demos, jobs] = results;
      if (status.status === 'rejected') throw status.reason;
      if (demos.status === 'rejected') throw demos.reason;
      if (jobs.status === 'rejected') throw jobs.reason;
      const demosById = new Map(demos.value.map((demo) => [demo.id, demo]));
      const jobsById = new Map(jobs.value.map((job) => [job.id, job]));
      for (const demo of pending.demos.values()) demosById.set(demo.id, demo);
      for (const job of pending.jobs.values()) jobsById.set(job.id, job);
      onSnapshot({
        status: status.value,
        demos: [...demosById.values()],
        jobs: [...jobsById.values()].sort((a, b) => b.createdAt.localeCompare(a.createdAt)),
      });
      succeeded = true;
    } catch (error) {
      if (!disposed) onError(error);
    } finally {
      updates = undefined;
      running = false;
      if (!disposed && ready) void run();
      else if (timer === undefined) settle();
    }
  }

  function dispose() {
    disposed = true;
    clearTimeout(timer);
    timer = undefined;
    unlisten?.();
    settle();
  }

  return { refresh, dispose };
}
