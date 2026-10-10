import type { Converted, Converter, ConverterOptions, Input, Limits } from './types.js';

export type { Converted, Converter, ConverterOptions, ErrorCode, Format, Input, Limits } from './types.js';
export { DEFAULT_LIMITS, IntoMdError } from './types.js';

export declare function createConverter(options?: ConverterOptions): Converter;

export declare function convert(input: Input, limits?: Partial<Limits>): Promise<Converted>;
