import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, join, posix } from 'node:path';
import { fileURLToPath } from 'node:url';

const ALLOWED_HOST = 'raw.githubusercontent.com';
const MAX_BYTES = 15 * 1024 * 1024;
const CONCURRENCY = 6;

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const manifestPath = join(root, 'scripts', 'corpus.json');
const corpusDir = join(root, 'test', 'corpus');
const writeHashes = process.argv.includes('--write-hashes');

const sha256 = (buf) => createHash('sha256').update(buf).digest('hex');

const targetOf = (entry) => {
  const name = posix.basename(entry.path);
  if (!/^[\w.~%()+-]+$/.test(name) || !/^[\w.-]+$/.test(entry.source)) {
    throw new Error(`unsafe name: ${entry.source}/${name}`);
  }
  return join(corpusDir, entry.source, name);
};

const urlOf = (entry) => {
  if (!/^[0-9a-f]{40}$/.test(entry.sha)) throw new Error(`sha must be a full commit SHA: ${entry.path}`);
  const encodedPath = entry.path.split('/').map(encodeURIComponent).join('/');
  return new URL(`https://${ALLOWED_HOST}/${entry.repo}/${entry.sha}/${encodedPath}`);
};

const download = async (url) => {
  const res = await fetch(url, { redirect: 'follow' });
  if (new URL(res.url).hostname !== ALLOWED_HOST) throw new Error(`redirected off ${ALLOWED_HOST}: ${res.url}`);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  const buf = Buffer.from(await res.arrayBuffer());
  if (buf.length > MAX_BYTES) throw new Error(`too large: ${buf.length} bytes`);
  return buf;
};

const readExisting = async (path) => {
  try {
    return await readFile(path);
  } catch {
    return null;
  }
};

const processEntry = async (entry) => {
  const target = targetOf(entry);
  const existing = await readExisting(target);
  if (existing && entry.sha256 && sha256(existing) === entry.sha256) return { status: 'cached', size: existing.length };

  const buf = await download(urlOf(entry));
  const hash = sha256(buf);
  if (entry.sha256 && entry.sha256 !== hash) throw new Error(`sha256 mismatch: expected ${entry.sha256}, got ${hash}`);
  if (!entry.sha256 && !writeHashes) throw new Error(`no sha256 in corpus.json (run with --write-hashes)`);

  await mkdir(dirname(target), { recursive: true });
  await writeFile(target, buf);
  if (!entry.sha256) entry.sha256 = hash;
  return { status: 'downloaded', size: buf.length };
};

const main = async () => {
  const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
  const targets = new Set();
  for (const entry of manifest) {
    const target = targetOf(entry);
    if (targets.has(target)) throw new Error(`duplicate target: ${target}`);
    targets.add(target);
  }

  const results = new Array(manifest.length);
  let next = 0;
  const worker = async () => {
    while (next < manifest.length) {
      const i = next++;
      const entry = manifest[i];
      try {
        results[i] = await processEntry(entry);
      } catch (err) {
        results[i] = { status: 'failed', error: err.message };
      }
      const r = results[i];
      console.log(`${r.status.padEnd(10)} ${entry.source}/${posix.basename(entry.path)}${r.error ? ` (${r.error})` : ''}`);
    }
  };
  await Promise.all(Array.from({ length: CONCURRENCY }, worker));

  if (writeHashes) await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);

  const counts = {};
  let total = 0;
  for (const [i, entry] of manifest.entries()) {
    const r = results[i];
    if (r.status === 'failed') continue;
    const key = `${entry.source} .${posix.extname(entry.path).slice(1).toLowerCase()}`;
    counts[key] = (counts[key] ?? 0) + 1;
    total += r.size;
  }
  const failed = results.filter((r) => r.status === 'failed').length;
  console.log('');
  for (const [key, n] of Object.entries(counts).sort()) console.log(`${String(n).padStart(4)}  ${key}`);
  console.log(`${manifest.length - failed}/${manifest.length} files, ${(total / 1024 / 1024).toFixed(1)} MB, ${failed} failed`);
  if (failed) process.exitCode = 1;
};

await main();
