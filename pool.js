import { IntoMdError, checkInputSize, toBytes } from './abi.js';

export function createPool({ workers, timeoutMs, spawn }) {
  const idle = [];
  const queue = [];
  const running = new Map();
  const dead = new WeakSet();
  let spawned = 0;
  let nextId = 0;
  let closed = false;

  const start = () => {
    const worker = spawn({
      message: ({ id, result, error }) => {
        const job = running.get(worker);
        if (!job || job.id !== id) return;
        finish(worker, job);
        if (error) job.reject(new IntoMdError(error.code, error.message));
        else job.resolve(result);
      },
      failure: (message) => fail(worker, new IntoMdError('malformed', message)),
    });
    spawned += 1;
    return worker;
  };

  const finish = (worker, job) => {
    clearTimeout(job.timer);
    running.delete(worker);
    worker.unref();
    idle.push(worker);
    dispatch();
  };

  const fail = (worker, error) => {
    if (dead.has(worker)) return;
    dead.add(worker);
    const job = running.get(worker);
    running.delete(worker);
    const index = idle.indexOf(worker);
    if (index !== -1) idle.splice(index, 1);
    spawned -= 1;
    if (job) {
      clearTimeout(job.timer);
      job.reject(error);
    }
    dispatch();
  };

  const dispatch = () => {
    while (queue.length > 0 && !closed) {
      const worker = idle.pop() ?? (spawned < workers ? start() : undefined);
      if (!worker) return;
      const job = queue.shift();
      running.set(worker, job);
      worker.ref();
      job.timer = setTimeout(() => {
        fail(worker, new IntoMdError('timeout', `conversion exceeded ${timeoutMs}ms`));
        worker.terminate();
      }, timeoutMs);
      worker.post({ id: job.id, bytes: job.bytes.buffer, limits: job.limits }, [job.bytes.buffer]);
    }
  };

  const convert = (input, limits = {}) =>
    new Promise((resolve, reject) => {
      if (closed) {
        reject(new Error('converter is closed'));
        return;
      }
      try {
        const bytes = toBytes(input);
        checkInputSize(bytes, limits);
        queue.push({ id: nextId++, bytes: new Uint8Array(bytes), limits, resolve, reject });
      } catch (error) {
        reject(error);
        return;
      }
      dispatch();
    });

  const close = async () => {
    closed = true;
    for (const job of queue.splice(0)) job.reject(new Error('converter is closed'));
    const all = [...idle.splice(0), ...running.keys()];
    await Promise.all(all.map((worker) => worker.terminate()));
  };

  return { convert, close };
}
