import { readFileSync } from 'node:fs';
import { Worker } from 'node:worker_threads';

import { DEFAULT_LIMITS, IntoMdError, checkInputSize, convertWithModule, needsFullModule, toBytes } from './abi.js';
import { createPool } from './pool.js';

const modules = new Map();

const loadModule = (name) => {
  if (!modules.has(name)) modules.set(name, new WebAssembly.Module(readFileSync(new URL(name, import.meta.url))));
  return modules.get(name);
};

export function convertSync(input, limits = {}) {
  const bytes = toBytes(input);
  checkInputSize(bytes, limits);
  return convertWithModule(loadModule(needsFullModule(bytes) ? './into_md_full.wasm' : './into_md.wasm'), bytes, limits);
}

export function createConverter({ workers = 2, timeoutMs = 30_000 } = {}) {
  return createPool({
    workers,
    timeoutMs,
    spawn: ({ message, failure }) => {
      const worker = new Worker(new URL('./node-worker.js', import.meta.url));
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
