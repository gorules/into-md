# @gorules/into-md

Converts DOCX, PPTX, XLSX, XLS, ODS, ODT, ODP, RTF and PDF to Markdown in the browser or Node.js: headings, bold/italic, lists, tables, hyperlinks and slide speaker notes.

```js
import { convert, createConverter } from '@gorules/into-md';

const { format, markdown } = await convert(await file.arrayBuffer());

const converter = createConverter({ workers: 1, timeoutMs: 30_000 });
await converter.convert(bytes, { maxMarkdownBytes: 1024 * 1024 });
await converter.close();
```

Conversions run in a Web Worker (browser) or worker thread (Node.js), so the page or event loop never blocks. Node.js additionally exports `convertSync`. Each conversion gets a fresh WebAssembly instance (memory capped at 512 MB), so a crash in one document never affects the next, and a conversion that exceeds `timeoutMs` has its worker terminated. Failures throw `IntoMdError` with `code` set to `unsupported`, `encrypted`, `too-large`, `malformed`, `timeout` or `unavailable` (module could not be loaded).

Two modules ship with the package:

| Module | Formats | Size (brotli) | Loaded |
| --- | --- | --- | --- |
| `into_md.wasm` | DOCX, PPTX, XLSX, ODT, ODP, ODS, RTF | ~115 KB | on first conversion |
| `into_md_full.wasm` | everything above plus PDF and legacy XLS | ~435 KB | only when a PDF or XLS file is converted |

Bundlers that understand `new Worker(new URL('./worker.js', import.meta.url))` and `new URL('./file.wasm', import.meta.url)` (Vite, webpack 5, Rollup, esbuild with plugins) pick up the worker and both modules automatically; no configuration is needed. RTF legacy code pages (Cyrillic, Shift-JIS, …) are decoded with the platform `TextDecoder`.

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
pnpm build         # size-optimized into_md.wasm + into_md_full.wasm (needs the nightly pinned in scripts/build.mjs, with rust-src)
pnpm test          # cargo tests + node tests against the built wasm
pnpm notices       # regenerate THIRD_PARTY_LICENSES.md after dependency changes
pnpm fixtures      # regenerate test/fixtures (cocoa.rtf comes from macOS textutil and is kept as is)
pnpm corpus        # download the pinned public corpus (Apache POI, python-docx, python-pptx, LibreOffice) into test/corpus
pnpm compare       # compare against officeparser on the corpus, writes test/corpus/report.json
```

## Releasing

Bump `version` in both `package.json` and `Cargo.toml`, commit, then push a matching `v<version>` tag. The Publish workflow validates, builds the wasm and runs `pnpm publish` with provenance using the `NPM_TOKEN` secret. `prepack` refuses to pack a stale or missing `into_md.wasm`, missing notices, or mismatched versions.

`vendor/pdf-extract` is pdf-extract 0.12.1 without lopdf's `wasm_js` feature, which otherwise pulls wasm-bindgen into the module. Features: `pdf` (pdf-extract), `xls` (calamine), `full` (both); `cargo test --release --features full` runs every test. The release build uses a pinned nightly with `-Z build-std` and `panic=immediate-abort`, which removes panic formatting from the binary.
