import { execFileSync } from 'node:child_process';
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';

const metadata = JSON.parse(
  execFileSync('cargo', ['metadata', '--format-version', '1', '--all-features', '--filter-platform', 'wasm32-unknown-unknown'], {
    cwd: new URL('..', import.meta.url).pathname,
    maxBuffer: 64 * 1024 * 1024,
  })
);

const packages = new Map(metadata.packages.map((pkg) => [pkg.id, pkg]));
const nodes = new Map(metadata.resolve.nodes.map((node) => [node.id, node]));
const root = metadata.packages.find((pkg) => pkg.name === 'into-md').id;
const included = new Set();
const pending = [root];
while (pending.length > 0) {
  const id = pending.pop();
  if (included.has(id)) continue;
  included.add(id);
  for (const dep of nodes.get(id).deps) {
    if (dep.dep_kinds.some((kind) => kind.kind === null)) pending.push(dep.pkg);
  }
}

const licenseFiles = (directory) =>
  readdirSync(directory)
    .filter((name) => /^(licen[cs]e|copying|notice|unlicense)/i.test(name))
    .sort()
    .map((name) => ({ name, text: readFileSync(join(directory, name), 'utf8').trim() }));

const crates = [...included]
  .filter((id) => id !== root)
  .map((id) => packages.get(id))
  .filter((pkg) => pkg.source !== null || pkg.name === 'into-md-pdf')
  .sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));

const texts = new Map();
const entries = crates.map((pkg) => {
  const files = licenseFiles(dirname(pkg.manifest_path));
  const references = files.map((file) => {
    const key = file.text.replace(/\s+/g, ' ');
    if (!texts.has(key)) texts.set(key, { id: `text-${texts.size + 1}`, title: file.name, text: file.text, crates: [] });
    const entry = texts.get(key);
    entry.crates.push(`${pkg.name} ${pkg.version}`);
    return `[${file.name}](#${entry.id})`;
  });
  return [
    `- **${pkg.name} ${pkg.version}**: ${pkg.license ?? 'see repository'}`,
    pkg.repository ? `, ${pkg.repository}` : '',
    references.length ? `. License files: ${references.join(', ')}` : '. No license file is shipped with this crate.',
  ].join('');
});

const output = [
  '# Third-party licenses',
  '',
  'into_md.wasm is compiled from the Rust crates below. Their license notices follow the list; identical texts are included once.',
  '',
  ...entries,
  '',
  ...[...texts.values()].flatMap((entry) => [
    `<a id="${entry.id}"></a>`,
    '',
    `## ${entry.title}`,
    '',
    `Used by: ${entry.crates.join(', ')}`,
    '',
    '```',
    entry.text,
    '```',
    '',
  ]),
].join('\n');

writeFileSync(new URL('../THIRD_PARTY_LICENSES.md', import.meta.url), output);
console.log(`${crates.length} crates, ${texts.size} distinct license texts written to THIRD_PARTY_LICENSES.md`);
