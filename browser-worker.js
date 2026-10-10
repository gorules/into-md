import { IntoMdError, convertWithModule, needsFullModule, serializeError } from './abi.js';

const sources = {
  core: new URL('./into_md.wasm', import.meta.url),
  full: new URL('./into_md_full.wasm', import.meta.url),
};

const modules = new Map();

const compile = async (url) => {
  const response = await fetch(url);
  if (!response.ok) throw new IntoMdError('unavailable', `could not load ${url}: HTTP ${response.status}`);
  if (response.headers.get('content-type')?.startsWith('application/wasm')) {
    return WebAssembly.compileStreaming(response);
  }
  return WebAssembly.compile(await response.arrayBuffer());
};

const loadModule = (kind) => {
  if (!modules.has(kind)) {
    const pending = compile(sources[kind]).catch((error) => {
      modules.delete(kind);
      throw error instanceof IntoMdError ? error : new IntoMdError('unavailable', `could not compile ${kind} module: ${error.message}`);
    });
    modules.set(kind, pending);
  }
  return modules.get(kind);
};

self.addEventListener('message', async ({ data: { id, bytes, limits } }) => {
  try {
    const view = new Uint8Array(bytes);
    const module = await loadModule(needsFullModule(view) ? 'full' : 'core');
    self.postMessage({ id, result: convertWithModule(module, view, limits) });
  } catch (error) {
    self.postMessage({ id, error: serializeError(error) });
  }
});
