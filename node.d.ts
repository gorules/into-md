import type { Converted, Input, Limits } from './types.js';

export * from './browser.js';

export declare function convertSync(input: Input, limits?: Partial<Limits>): Converted;
