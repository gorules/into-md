import { execFileSync } from 'node:child_process';
import { copyFileSync } from 'node:fs';

export const TOOLCHAIN = 'nightly-2026-10-10';

export const MODULES = [{ output: 'into_md.wasm', features: [] }];

const RUSTFLAGS = [
  '--cfg getrandom_backend="custom"',
  '-C link-arg=--max-memory=536870912',
  '-Z unstable-options',
  '-C panic=immediate-abort',
].join(' ');

const root = new URL('..', import.meta.url);

export const builtPath = (output) =>
  new URL(`target/${output.replace('.wasm', '')}/wasm32-unknown-unknown/release/into_md.wasm`, root);

if (import.meta.url === `file://${process.argv[1]}`) {
  for (const { output, features } of MODULES) {
    execFileSync(
      'cargo',
      [
        `+${process.env.INTO_MD_TOOLCHAIN ?? TOOLCHAIN}`,
        'build',
        '--package',
        'into-md',
        '--release',
        '--target',
        'wasm32-unknown-unknown',
        '--target-dir',
        `target/${output.replace('.wasm', '')}`,
        '-Z',
        'build-std=std,panic_abort',
        '-Z',
        'build-std-features=optimize_for_size',
        ...features,
      ],
      { cwd: root, stdio: 'inherit', env: { ...process.env, RUSTFLAGS } }
    );
    copyFileSync(builtPath(output), new URL(output, root));
  }
}
