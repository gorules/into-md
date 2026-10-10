import { existsSync, readFileSync, statSync } from 'node:fs';

import { MODULES, builtPath } from './build.mjs';

const root = new URL('..', import.meta.url);

const fail = (message) => {
  console.error(`check-package: ${message}`);
  process.exit(1);
};

if (!existsSync(new URL('THIRD_PARTY_LICENSES.md', root))) fail('THIRD_PARTY_LICENSES.md is missing, run `pnpm notices`');

const REQUIRED_EXPORTS = ['memory', 'into_md_alloc', 'into_md_convert', 'into_md_format', 'into_md_output_ptr', 'into_md_output_len'];
const ALLOWED_IMPORTS = new Set(['into_md.decode']);

for (const { output } of MODULES) {
  const wasmPath = new URL(output, root);
  if (!existsSync(wasmPath)) fail(`${output} is missing, run \`pnpm build\` first`);
  const built = builtPath(output);
  if (existsSync(built) && statSync(built).mtimeMs > statSync(wasmPath).mtimeMs) {
    fail(`${output} is older than the cargo build output, run \`pnpm build\``);
  }
  const module = new WebAssembly.Module(readFileSync(wasmPath));
  const imports = WebAssembly.Module.imports(module).map((entry) => `${entry.module}.${entry.name}`);
  const unexpected = imports.filter((name) => !ALLOWED_IMPORTS.has(name));
  if (unexpected.length > 0) fail(`${output} has unexpected imports: ${unexpected.join(', ')}`);
  const exported = new Set(WebAssembly.Module.exports(module).map((entry) => entry.name));
  for (const name of REQUIRED_EXPORTS) {
    if (!exported.has(name)) fail(`${output} is missing the ${name} export`);
  }
  console.log(`check-package: ${output} ok (${statSync(wasmPath).size} bytes)`);
}

const { version } = JSON.parse(readFileSync(new URL('package.json', root), 'utf8'));
const cargoVersion = readFileSync(new URL('Cargo.toml', root), 'utf8').match(/^version = "([^"]+)"/m)?.[1];
if (cargoVersion !== version) fail(`package.json version ${version} does not match Cargo.toml version ${cargoVersion}`);
