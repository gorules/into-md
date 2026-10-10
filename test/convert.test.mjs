import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { test } from 'node:test';

import { IntoMdError, convert, convertSync, createConverter } from '../node.js';

const require = createRequire(import.meta.url);
const JSZip = require('jszip');

const fixture = (name) => readFile(new URL(`fixtures/${name}`, import.meta.url));

test('converts every fixture format', async () => {
  const expected = {
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

test('reports typed errors', async () => {
  await assert.rejects(convert(new TextEncoder().encode('plain text')), { name: 'IntoMdError', code: 'unsupported' });
  const bomb = await new JSZip()
    .file('word/document.xml', ' '.repeat(30 * 1024 * 1024))
    .generateAsync({ type: 'uint8array', compression: 'DEFLATE' });
  await assert.rejects(convert(bomb), { code: 'too-large', message: 'uncompressed size limit exceeded' });
  await assert.rejects(convert(await fixture('policy.docx'), { maxMarkdownBytes: 10 }), { code: 'too-large' });
  await assert.rejects(convert(new Uint8Array(30 * 1024 * 1024)), { code: 'too-large', message: 'input limit exceeded' });
});

test('turns engine panics into errors and keeps working afterwards', async () => {
  await assert.rejects(convert(await fixture('panics.pdf')), { name: 'IntoMdError', code: 'malformed', message: /^conversion aborted/ });
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

test('convertSync runs on the calling thread', async () => {
  assert.equal(convertSync(await fixture('deck.odp')).format, 'odp');
  assert.throws(() => convertSync(new Uint8Array([1, 2, 3])), { code: 'unsupported' });
});

test('queues beyond the worker count and times out stuck conversions', async () => {
  const converter = createConverter({ workers: 2 });
  const docx = await fixture('policy.docx');
  const results = await Promise.all(Array.from({ length: 12 }, () => converter.convert(docx)));
  assert.ok(results.every((result) => result.format === 'docx'));
  await converter.close();
  await assert.rejects(converter.convert(docx), /closed/);

  const impatient = createConverter({ workers: 1, timeoutMs: 1 });
  await assert.rejects(impatient.convert(docx), { code: 'timeout' });
  await impatient.close();
});

test('does not mutate or detach the caller buffer', async () => {
  const bytes = await fixture('policy.docx');
  const copy = Buffer.from(bytes);
  await convert(bytes);
  assert.equal(bytes.byteLength, copy.byteLength);
  assert.ok(bytes.equals(copy));
});

test('decodes legacy RTF code pages through TextDecoder', () => {
  const rtf = String.raw`{\rtf1\ansi\ansicpg1251{\fonttbl{\f0\fcharset204 Arial;}{\f1\fcharset128 MS Mincho;}{\f2\fcharset0 Arial;}}\f0 \'cf\'f0\'e8\'e2\'e5\'f2 {\f1 \'93\'fa\'96\'7b} {\f2 caf\'e9 \'80}\par}`;
  const { format, markdown } = convertSync(new TextEncoder().encode(rtf));
  assert.equal(format, 'rtf');
  assert.equal(markdown, 'Привет 日本 café €');
});

test('entry points export exactly what the type declarations describe', async () => {
  const node = await import('../node.js');
  const browser = await import('../browser.js');
  assert.deepEqual(Object.keys(node).sort(), ['DEFAULT_LIMITS', 'IntoMdError', 'convert', 'convertSync', 'createConverter']);
  assert.deepEqual(Object.keys(browser).sort(), ['DEFAULT_LIMITS', 'IntoMdError', 'convert', 'createConverter']);
  const declared = (file) =>
    [...readFileSync(new URL(`../${file}`, import.meta.url), 'utf8').matchAll(/export declare (?:function|const|class) (\w+)/g)].map((match) => match[1]);
  assert.deepEqual(declared('types.d.ts').sort(), ['DEFAULT_LIMITS', 'IntoMdError']);
  assert.deepEqual(declared('browser.d.ts').sort(), ['convert', 'createConverter']);
  assert.deepEqual(declared('node.d.ts'), ['convertSync']);
});
