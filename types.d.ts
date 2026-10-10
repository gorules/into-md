export type Format = 'docx' | 'pptx' | 'xlsx' | 'xls' | 'ods' | 'odt' | 'odp' | 'pdf' | 'rtf';

export type ErrorCode = 'unsupported' | 'encrypted' | 'too-large' | 'malformed' | 'timeout' | 'unavailable';

export interface Limits {
  maxInputBytes: number;
  maxUncompressedBytes: number;
  maxZipEntries: number;
  maxXmlElements: number;
  maxTableCells: number;
  maxMarkdownBytes: number;
}

export interface Converted {
  format: Format;
  markdown: string;
}

export type Input = Uint8Array | ArrayBuffer | ArrayBufferView;

export interface ConverterOptions {
  workers?: number;
  timeoutMs?: number;
}

export interface Converter {
  convert(input: Input, limits?: Partial<Limits>): Promise<Converted>;
  close(): Promise<void>;
}

export declare const DEFAULT_LIMITS: Readonly<Limits>;

export declare class IntoMdError extends Error {
  readonly code: ErrorCode;
}
