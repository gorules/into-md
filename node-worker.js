import { parentPort } from 'node:worker_threads';

import { serializeError } from './abi.js';
import { convertSync } from './node.js';

parentPort.on('message', ({ id, bytes, limits }) => {
  try {
    parentPort.postMessage({ id, result: convertSync(new Uint8Array(bytes), limits) });
  } catch (error) {
    parentPort.postMessage({ id, error: serializeError(error) });
  }
});
