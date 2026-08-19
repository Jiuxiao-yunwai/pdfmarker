export interface QueuedPdfRender {
  promise: Promise<boolean>;
  cancel: () => void;
  reprioritize: (priority: number) => void;
}

interface RenderJob {
  cancelled: boolean;
  priority: number;
  sequence: number;
  started: boolean;
  run: () => Promise<void>;
  resolve: (completed: boolean) => void;
  reject: (reason: unknown) => void;
}

const pendingJobs: RenderJob[] = [];
let activeJob: RenderJob | undefined;
let nextSequence = 0;

function sortPendingJobs() {
  pendingJobs.sort((left, right) => left.priority - right.priority || left.sequence - right.sequence);
}

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

export function queuePdfRender(run: () => Promise<void>, priority = Number.MAX_SAFE_INTEGER): QueuedPdfRender {
  let resolve!: (completed: boolean) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<boolean>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  const job: RenderJob = {
    cancelled: false,
    priority,
    sequence: nextSequence++,
    started: false,
    run,
    resolve,
    reject,
  };
  pendingJobs.push(job);
  sortPendingJobs();
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
    reprioritize: (nextPriority) => {
      if (job.cancelled || job.started || job.priority === nextPriority) return;
      job.priority = nextPriority;
      sortPendingJobs();
    },
  };
}
