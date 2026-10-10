import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { test } from 'node:test';

import { IntoMdError, convert, createConverter } from '@gorules/into-md';
import type { ErrorCode, Format } from '@gorules/into-md';
import JSZip from 'jszip';

const fixture = (name: string): Promise<Buffer> => readFile(new URL(`fixtures/${name}`, import.meta.url));

const rejectsWith = (code: ErrorCode, message?: string | RegExp) => (error: unknown) => {
  assert.ok(error instanceof IntoMdError, `expected IntoMdError, got ${String(error)}`);
  assert.equal(error.code, code);
  if (typeof message === 'string') assert.equal(error.message, message);
  if (message instanceof RegExp) assert.match(error.message, message);
  return true;
};

test('converts every fixture format', async () => {
  const expected: Record<string, [Format, string]> = {
    'policy.docx': ['docx', '# Loan Policy'],
    'policy.odt': ['odt', '## Eligibility'],
    'rates.xlsx': ['xlsx', '| A | 750 | 0.035 | 2026-01-15 |'],
    'rates.ods': ['ods', '| A | 750 |'],
    'deck.pptx': ['pptx', '[pricing link](https://gorules.io/pricing)'],
    'deck.odp': ['odp', '## Slide 2: Pricing'],
    'policy.pdf': ['pdf', 'Income must exceed 30k.'],
    'policy.rtf': ['rtf', '## Eligibility'],
    'cocoa.rtf': ['rtf', 'Café — naïve'],
  };
  for (const [name, [format, snippet]] of Object.entries(expected)) {
    const result = await convert(await fixture(name));
    assert.equal(result.format, format, name);
    assert.ok(result.markdown.includes(snippet), `${name}: ${result.markdown}`);
  }
});

test('accepts ArrayBuffer input', async () => {
  const bytes = await fixture('rates.ods');
  const result = await convert(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength));
  assert.equal(result.format, 'ods');
});

test('accepts SharedArrayBuffer input', async () => {
  const bytes = await fixture('deck.odp');
  const shared = new SharedArrayBuffer(bytes.byteLength);
  new Uint8Array(shared).set(bytes);
  assert.equal((await convert(shared)).format, 'odp');
});

test('rejects non-binary input with a TypeError', async () => {
  await assert.rejects(convert('not bytes' as unknown as Uint8Array), TypeError);
});

test('reports typed errors', async () => {
  await assert.rejects(convert(new TextEncoder().encode('plain text')), rejectsWith('unsupported'));
  const bomb = await new JSZip()
    .file('word/document.xml', ' '.repeat(30 * 1024 * 1024))
    .generateAsync({ type: 'uint8array', compression: 'DEFLATE' });
  await assert.rejects(convert(bomb), rejectsWith('too-large', 'uncompressed size limit exceeded'));
  await assert.rejects(convert(await fixture('policy.docx'), { maxMarkdownBytes: 10 }), rejectsWith('too-large'));
  await assert.rejects(convert(new Uint8Array(30 * 1024 * 1024)), rejectsWith('too-large', 'input limit exceeded'));
});

test('turns engine panics into errors and keeps working afterwards', async () => {
  await assert.rejects(convert(await fixture('panics.pdf')), rejectsWith('malformed', /^conversion aborted/));
  assert.equal((await convert(await fixture('policy.docx'))).format, 'docx');
});

test('converts concurrently with isolated instances', async () => {
  const names = ['policy.docx', 'rates.xlsx', 'deck.pptx', 'policy.pdf'];
  const results = await Promise.all(names.map(async (name) => convert(await fixture(name))));
  assert.deepEqual(
    results.map((result) => result.format),
    ['docx', 'xlsx', 'pptx', 'pdf']
  );
});

test('queues beyond the worker count and times out stuck conversions', async () => {
  const converter = createConverter({ workers: 2 });
  const docx = await fixture('policy.docx');
  const results = await Promise.all(Array.from({ length: 12 }, () => converter.convert(docx)));
  assert.ok(results.every((result) => result.format === 'docx'));
  await converter.close();
  await assert.rejects(converter.convert(docx), /closed/);

  const impatient = createConverter({ workers: 1, timeoutMs: 1 });
  await assert.rejects(impatient.convert(docx), rejectsWith('timeout'));
  await impatient.close();
});

test('does not mutate or detach the caller buffer', async () => {
  const bytes = await fixture('policy.docx');
  const copy = Buffer.from(bytes);
  await convert(bytes);
  assert.equal(bytes.byteLength, copy.byteLength);
  assert.ok(bytes.equals(copy));
});

test('decodes legacy RTF code pages through TextDecoder', async () => {
  const rtf = String.raw`{\rtf1\ansi\ansicpg1251{\fonttbl{\f0\fcharset204 Arial;}{\f1\fcharset128 MS Mincho;}{\f2\fcharset0 Arial;}}\f0 \'cf\'f0\'e8\'e2\'e5\'f2 {\f1 \'93\'fa\'96\'7b} {\f2 caf\'e9 \'80}\par}`;
  const { format, markdown } = await convert(new TextEncoder().encode(rtf));
  assert.equal(format, 'rtf');
  assert.equal(markdown, 'Привет 日本 café €');
});

test('entry points export exactly what the type declarations describe', async () => {
  const declared = [...readFileSync(new URL('../index.d.ts', import.meta.url), 'utf8').matchAll(/export declare (?:function|const|class) (\w+)/g)]
    .map((match) => match[1])
    .sort();
  assert.deepEqual(declared, ['DEFAULT_LIMITS', 'IntoMdError', 'convert', 'createConverter']);
  const entries: Record<string, Record<string, unknown>> = {
    node: await import(new URL('../node.js', import.meta.url).href),
    browser: await import(new URL('../browser.js', import.meta.url).href),
  };
  for (const [name, entry] of Object.entries(entries)) {
    assert.deepEqual(Object.keys(entry).sort(), declared, name);
  }
});

test('importing and creating a converter starts nothing until the first conversion', async () => {
  const script = `
    const { readFileSync } = await import('node:fs');
    const { convert, createConverter } = await import(${JSON.stringify(new URL('../node.js', import.meta.url).href)});
    const converter = createConverter();
    const idle = process.getActiveResourcesInfo();
    const pending = convert(readFileSync(${JSON.stringify(new URL('fixtures/policy.docx', import.meta.url).pathname)}));
    const busy = process.getActiveResourcesInfo();
    const { format } = await pending;
    await converter.close();
    console.log(JSON.stringify({ idle, busy: busy.includes('MessagePort'), format }));
  `;
  const output: unknown = JSON.parse(execFileSync(process.execPath, ['--input-type=module', '-e', script], { encoding: 'utf8' }));
  assert.deepEqual(output, { idle: [], busy: true, format: 'docx' });
});
