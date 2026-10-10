# @gorules/into-md

Converts DOCX, PPTX, XLSX, XLS (Excel 97–2003), ODS, ODT, ODP, RTF and PDF to Markdown in the browser or Node.js: headings, bold/italic, lists, tables, hyperlinks and slide speaker notes.

```js
import { convert, createConverter } from '@gorules/into-md';

const { format, markdown } = await convert(await file.arrayBuffer());

const converter = createConverter({ workers: 1, timeoutMs: 30_000 });
await converter.convert(bytes, { maxMarkdownBytes: 1024 * 1024 });
await converter.close();
```

Nothing is loaded at import time: the first `convert` starts a Web Worker (browser) or worker thread (Node.js), which fetches and compiles the WebAssembly module, so the page or event loop never blocks. Each conversion gets a fresh WebAssembly instance (memory capped at 512 MB), so a crash in one document never affects the next, and a conversion that exceeds `timeoutMs` has its worker terminated. Failures throw `IntoMdError` with `code` set to `unsupported`, `encrypted`, `too-large`, `malformed`, `timeout` or `unavailable` (module could not be loaded).

The package ships a single module, `into_md.wasm` (about 620 KB, 225 KB brotli), fetched and compiled by the worker on the first conversion.

Bundlers that understand `new Worker(new URL('./worker.js', import.meta.url))` and `new URL('./file.wasm', import.meta.url)` (Vite, webpack 5, Rollup, esbuild with plugins) pick up the worker and the module automatically; no configuration is needed. RTF legacy code pages (Cyrillic, Shift-JIS, …) are decoded with the platform `TextDecoder`.

Limits (all optional, defaults shown):

| Option | Default |
| --- | --- |
| `maxInputBytes` | 25 MB |
| `maxUncompressedBytes` | 20 MB, measured by inflating every zip entry before parsing |
| `maxZipEntries` | 2000 |
| `maxXmlElements` | 500,000 |
| `maxTableCells` | 200,000 |
| `maxMarkdownBytes` | 2 MB |

## Development

```sh
pnpm install
pnpm build         # size-optimized into_md.wasm (needs the nightly pinned in scripts/build.mjs, with rust-src)
pnpm test          # cargo tests + node tests against the built wasm
pnpm notices       # regenerate THIRD_PARTY_LICENSES.md after dependency changes
pnpm fixtures      # regenerate test/fixtures (cocoa.rtf comes from macOS textutil and is kept as is)
pnpm corpus        # download the pinned public corpus (Apache POI, python-docx, python-pptx, LibreOffice, pdf.js) into test/corpus
pnpm compare       # compare against officeparser on the corpus, writes test/corpus/report.json
```

## Releasing

Releases are managed by [release-please](https://github.com/googleapis/release-please) from [Conventional Commits](https://www.conventionalcommits.org/) on `main` (`feat:` bumps the minor version before 1.0, `fix:` the patch version). It keeps a release PR open that bumps `package.json`, `crates/into-md/Cargo.toml`, `Cargo.lock` and `CHANGELOG.md`; merging it tags the release and the Release workflow validates, builds the wasm and runs `pnpm publish` with provenance. The workflow needs the `PAT` and `NPM_TOKEN` secrets. `prepack` refuses to pack a stale or missing `into_md.wasm`, missing notices, or mismatched versions.

The Rust workspace lives in `crates/`:

| Crate | Purpose |
| --- | --- |
| `crates/into-md` | the converter compiled to `into_md.wasm`; the `pdf` feature (on by default) adds PDF support |
| `crates/pdf` | `into-md-pdf`, a size-reduced fork of pdf-extract 0.12.1 (see its README for what changed) |
| `crates/utf16` | stands in for `encoding_rs` (UTF-16 only), which is all lopdf and the PDF fork need |
| `crates/saslprep` | stands in for `stringprep`; PDFs are only opened with the empty password |

The release build uses a pinned nightly with `-Z build-std` and `panic=immediate-abort`, which removes panic formatting from the binary. Legacy code pages (RTF, old spreadsheets) are decoded by the host's `TextDecoder`; native builds fall back to Windows-1252.
