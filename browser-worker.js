import { IntoMdError, convertWithModule, serializeError } from './abi.js';

const source = new URL('./into_md.wasm', import.meta.url);

let pending;

const compile = async () => {
  const response = await fetch(source);
  if (!response.ok) throw new IntoMdError('unavailable', `could not load ${source}: HTTP ${response.status}`);
  if (response.headers.get('content-type')?.startsWith('application/wasm')) {
    return WebAssembly.compileStreaming(response);
  }
  return WebAssembly.compile(await response.arrayBuffer());
};

const loadModule = () =>
  (pending ??= compile().catch((error) => {
    pending = undefined;
    throw error instanceof IntoMdError ? error : new IntoMdError('unavailable', `could not compile ${source}: ${error.message}`);
  }));

self.addEventListener('message', async ({ data: { id, bytes, limits } }) => {
  try {
    self.postMessage({ id, result: convertWithModule(await loadModule(), new Uint8Array(bytes), limits) });
  } catch (error) {
    self.postMessage({ id, error: serializeError(error) });
  }
});
