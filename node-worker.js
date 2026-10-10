import { readFileSync } from 'node:fs';
import { parentPort } from 'node:worker_threads';

import { convertWithModule, serializeError } from './abi.js';

let module;

const loadModule = () => (module ??= new WebAssembly.Module(readFileSync(new URL('./into_md.wasm', import.meta.url))));

parentPort.on('message', ({ id, bytes, limits }) => {
  try {
    parentPort.postMessage({ id, result: convertWithModule(loadModule(), new Uint8Array(bytes), limits) });
  } catch (error) {
    parentPort.postMessage({ id, error: serializeError(error) });
  }
});
