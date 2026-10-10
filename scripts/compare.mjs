import { readFile, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { basename, extname } from 'node:path';

import { createConverter } from '../node.js';

const require = createRequire(import.meta.url);
const { OfficeParser } = require('officeparser');
const corpus = require('./corpus.json');

const words = (text) => new Set(text.toLowerCase().match(/[\p{L}\p{N}]{3,}/gu) ?? []);
const coverage = (from, into) => {
  if (from.size === 0) return 1;
  let hits = 0;
  for (const word of from) if (into.has(word)) hits += 1;
  return hits / from.size;
};

const officeparser = async (bytes) => {
  const started = performance.now();
  try {
    const ast = await OfficeParser.parseOffice(bytes, { ocr: false, extractAttachments: false });
    const { value } = await ast.to('md', { generateIds: false });
    return { ok: true, markdown: value.replace(/^---[\s\S]*?---\s*/, ''), ms: performance.now() - started };
  } catch (error) {
    return { ok: false, error: String(error.message).slice(0, 80), ms: performance.now() - started };
  }
};

const converter = createConverter({ workers: 4, timeoutMs: 20_000 });
const rows = [];
await Promise.all(
  corpus.map(async (entry) => {
    const file = new URL(`../test/corpus/${entry.source}/${basename(entry.path)}`, import.meta.url);
    const bytes = await readFile(file);
    const started = performance.now();
    let ours;
    try {
      const result = await converter.convert(bytes, { maxMarkdownBytes: 64 << 20, maxTableCells: 5_000_000 });
      ours = { ok: true, markdown: result.markdown, format: result.format };
    } catch (error) {
      ours = { ok: false, code: error.code, error: String(error.message).slice(0, 80) };
    }
    ours.ms = performance.now() - started;
    const theirs = await officeparser(bytes);
    const a = words(ours.markdown ?? '');
    const b = words(theirs.markdown ?? '');
    rows.push({
      file: `${entry.source}/${basename(entry.path)}`,
      ext: extname(entry.path).slice(1).toLowerCase(),
      kb: Math.round(bytes.length / 1024),
      expect: entry.expect ?? 'ok',
      ours: ours.ok ? 'ok' : `${ours.code}: ${ours.error}`,
      theirs: theirs.ok ? 'ok' : `error: ${theirs.error}`,
      ourWords: a.size,
      theirWords: b.size,
      weCoverThem: Number(coverage(b, a).toFixed(3)),
      theyCoverUs: Number(coverage(a, b).toFixed(3)),
      ourMs: Math.round(ours.ms),
      theirMs: Math.round(theirs.ms),
      markdown: ours.markdown ?? '',
    });
  })
);
await converter.close();
rows.sort((x, y) => x.file.localeCompare(y.file));
await writeFile(new URL('../test/corpus/report.json', import.meta.url), JSON.stringify(rows, null, 2));

const byExt = {};
for (const row of rows) {
  const bucket = (byExt[row.ext] ??= { files: 0, oursOk: 0, theirsOk: 0, bothOk: 0, coverage: [], ourMs: 0, theirMs: 0 });
  bucket.files += 1;
  if (row.ours === 'ok') bucket.oursOk += 1;
  if (row.theirs === 'ok') bucket.theirsOk += 1;
  if (row.ours === 'ok' && row.theirs === 'ok') {
    bucket.bothOk += 1;
    bucket.coverage.push(row.weCoverThem);
  }
  bucket.ourMs += row.ourMs;
  bucket.theirMs += row.theirMs;
}
for (const [ext, bucket] of Object.entries(byExt)) {
  const sorted = bucket.coverage.sort((x, y) => x - y);
  const median = sorted.length ? sorted[Math.floor(sorted.length / 2)] : null;
  console.log(
    ext.padEnd(5),
    `files ${bucket.files}`.padEnd(9),
    `into-md ok ${bucket.oursOk}`.padEnd(14),
    `officeparser ok ${bucket.theirsOk}`.padEnd(19),
    `coverage median ${median} min ${sorted[0] ?? null}`.padEnd(32),
    `time ${bucket.ourMs}ms vs ${bucket.theirMs}ms`
  );
}
console.log('\nlow coverage (<0.9) or only one side failing:');
for (const row of rows) {
  const oneSided = (row.ours === 'ok') !== (row.theirs === 'ok');
  if (oneSided || (row.ours === 'ok' && row.theirs === 'ok' && row.weCoverThem < 0.9 && row.theirWords > 20)) {
    console.log(' ', row.file, `[expect ${row.expect}]`, '| ours:', row.ours, '| theirs:', row.theirs, `| cover ${row.weCoverThem}/${row.theyCoverUs} words ${row.ourWords}/${row.theirWords}`);
  }
}
