export interface QueuedPdfRender {
  promise: Promise<boolean>;
  cancel: () => void;
}

interface RenderJob {
  cancelled: boolean;
  started: boolean;
  run: () => Promise<void>;
  resolve: (completed: boolean) => void;
  reject: (reason: unknown) => void;
}

const pendingJobs: RenderJob[] = [];
let activeJob: RenderJob | undefined;

function runNextJob() {
  if (activeJob) return;

  const job = pendingJobs.shift();
  if (!job) return;
  if (job.cancelled) {
    job.resolve(false);
    runNextJob();
    return;
  }

  activeJob = job;
  job.started = true;
  void job.run()
    .then(() => job.resolve(!job.cancelled), job.reject)
    .finally(() => {
      activeJob = undefined;
      runNextJob();
    });
}

export function queuePdfRender(run: () => Promise<void>): QueuedPdfRender {
  let resolve!: (completed: boolean) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<boolean>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  const job: RenderJob = { cancelled: false, started: false, run, resolve, reject };
  pendingJobs.push(job);
  runNextJob();

  return {
    promise,
    cancel: () => {
      if (job.cancelled) return;
      job.cancelled = true;
      if (job.started) return;
      const index = pendingJobs.indexOf(job);
      if (index >= 0) pendingJobs.splice(index, 1);
      job.resolve(false);
    },
  };
}
