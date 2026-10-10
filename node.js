import { Worker } from 'node:worker_threads';

import { DEFAULT_LIMITS, IntoMdError } from './abi.js';
import { createPool } from './pool.js';

const WORKER_INCOMPATIBLE_FLAGS = ['--input-type', '--eval', '-e', '--print', '-p'];

const workerOptions = () => {
  const execArgv = process.execArgv.filter(
    (flag) => !WORKER_INCOMPATIBLE_FLAGS.some((name) => flag === name || flag.startsWith(`${name}=`))
  );
  return execArgv.length === process.execArgv.length ? {} : { execArgv };
};

export function createConverter({ workers = 2, timeoutMs = 30_000 } = {}) {
  return createPool({
    workers,
    timeoutMs,
    spawn: ({ message, failure }) => {
      const worker = new Worker(new URL('./node-worker.js', import.meta.url), workerOptions());
      worker.on('message', message);
      worker.on('error', (error) => failure(`worker crashed: ${error.message}`));
      worker.on('exit', () => failure('worker exited'));
      return {
        post: (data, transfer) => worker.postMessage(data, transfer),
        terminate: () => worker.terminate(),
        ref: () => worker.ref(),
        unref: () => worker.unref(),
      };
    },
  });
}

let shared;

export const convert = (input, limits) => (shared ??= createConverter()).convert(input, limits);

export { DEFAULT_LIMITS, IntoMdError };
