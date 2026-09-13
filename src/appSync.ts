import type { api, AppEvent, AnalysisJob, DemoMeta, RenderJob, Status } from './api.ts';

const REFRESH_DEBOUNCE_MS = 300;
interface Snapshot { status: Status; demos: DemoMeta[]; jobs: RenderJob[]; analysisJobs: AnalysisJob[] }

export function createAppSync(
  backend: Pick<typeof api, 'onEvent' | 'status' | 'demos' | 'jobs' | 'analysisJobs'>,
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
  let updates: { demos: Map<string, DemoMeta>; jobs: Map<string, RenderJob>; analysisJobs: Map<string, AnalysisJob> } | undefined;

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
          if (event.type === 'analysis-job-changed') {
            const old=updates?.analysisJobs.get(event.job.id);
            if (!old || event.job.revision>=old.revision) updates?.analysisJobs.set(event.job.id,event.job);
          }
          onEvent(event);
          if (event.type === 'setup-finished') void refresh();
        });
        if (disposed) { stop(); return; }
        unlisten = stop;
      }
      const pending = { demos: new Map<string, DemoMeta>(), jobs: new Map<string, RenderJob>(), analysisJobs: new Map<string, AnalysisJob>() };
      updates = pending;
      // Wait for every scan, including when one fails, before starting another pass.
      const results = await Promise.allSettled([backend.status(), backend.demos(), backend.jobs(), backend.analysisJobs()]);
      if (disposed) return;
      const [status, demos, jobs, analysisJobs] = results;
      if (status.status === 'rejected') throw status.reason;
      if (demos.status === 'rejected') throw demos.reason;
      if (jobs.status === 'rejected') throw jobs.reason;
      if (analysisJobs.status === 'rejected') throw analysisJobs.reason;
      const demosById = new Map(demos.value.map((demo) => [demo.id, demo]));
      const jobsById = new Map(jobs.value.map((job) => [job.id, job]));
      for (const demo of pending.demos.values()) demosById.set(demo.id, demo);
      for (const job of pending.jobs.values()) jobsById.set(job.id, job);
      const analysisById=new Map(analysisJobs.value.map(job=>[job.id,job]));
      for (const job of pending.analysisJobs.values()) {
        if (job.revision >= (analysisById.get(job.id)?.revision ?? -1)) analysisById.set(job.id,job);
      }
      onSnapshot({
        status: status.value,
        analysisJobs: [...analysisById.values()].sort((a,b)=>b.sequence-a.sequence),
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
