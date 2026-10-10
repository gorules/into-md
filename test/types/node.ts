import * as intoMd from '@gorules/into-md';
import { DEFAULT_LIMITS, IntoMdError, convert, createConverter } from '@gorules/into-md';
import type { Converted, Limits } from '@gorules/into-md';

import { codes, describe, limits, useConverter } from './shared.js';

const bytes = new Uint8Array([1, 2, 3]);

const converted: Converted = await convert(bytes, limits);
const viaBuffer: Promise<Converted> = convert(Buffer.from(bytes));
const viaArrayBuffer: Promise<Converted> = convert(bytes.buffer);
const viaView: Promise<Converted> = convert(new DataView(bytes.buffer));
const pooled: Promise<string> = useConverter(createConverter({ workers: 2, timeoutMs: 1000 }), bytes);
const defaults: Readonly<Limits> = DEFAULT_LIMITS;

try {
  describe(converted);
} catch (error) {
  if (error instanceof IntoMdError) {
    const code: (typeof codes)[number] = error.code;
    const message: string = error.message;
    void [code, message];
  }
}

// @ts-expect-error input must be binary
convert('text');
// @ts-expect-error there is no synchronous API
intoMd.convertSync(bytes);
// @ts-expect-error unknown limit
convert(bytes, { maxPages: 3 });
// @ts-expect-error unknown converter option
createConverter({ threads: 2 });
// @ts-expect-error read-only defaults
DEFAULT_LIMITS.maxInputBytes = 1;
// @ts-expect-error error codes are a closed set
const wrong: IntoMdError['code'] = 'oops';

void [viaBuffer, viaArrayBuffer, viaView, pooled, defaults, wrong];
