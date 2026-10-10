import * as intoMd from '@gorules/into-md';
import { DEFAULT_LIMITS, IntoMdError, convert, createConverter } from '@gorules/into-md';
import type { Converted, Limits } from '@gorules/into-md';

import { limits, useConverter } from './shared.js';

const bytes = new Uint8Array([1, 2, 3]);

const converted: Promise<Converted> = convert(bytes, limits);
const pooled: Promise<string> = useConverter(createConverter({ workers: 1 }), bytes);
const defaults: Readonly<Limits> = DEFAULT_LIMITS;
const isError = (value: unknown): value is IntoMdError => value instanceof IntoMdError;

// @ts-expect-error convertSync is Node.js only
intoMd.convertSync(bytes);

void [converted, pooled, defaults, isError];
