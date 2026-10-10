import { DEFAULT_LIMITS, IntoMdError } from './abi.js';
import { createPool } from './pool.js';

export function createConverter({ workers = 1, timeoutMs = 30_000 } = {}) {
  return createPool({
    workers,
    timeoutMs,
    spawn: ({ message, failure }) => {
      const worker = new Worker(new URL('./browser-worker.js', import.meta.url), { type: 'module' });
      worker.addEventListener('message', (event) => message(event.data));
      worker.addEventListener('error', (event) => {
        event.preventDefault();
        failure(`worker crashed: ${event.message ?? 'unknown error'}`);
      });
      worker.addEventListener('messageerror', () => failure('worker message could not be deserialized'));
      return {
        post: (data, transfer) => worker.postMessage(data, transfer),
        terminate: () => worker.terminate(),
        ref: () => {},
        unref: () => {},
      };
    },
  });
}

let shared;

export const convert = (input, limits) => (shared ??= createConverter()).convert(input, limits);

export { DEFAULT_LIMITS, IntoMdError };
