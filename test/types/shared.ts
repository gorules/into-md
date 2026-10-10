import type { Converted, Converter, ErrorCode, Format, Limits } from '@gorules/into-md';

export const formats: Format[] = ['docx', 'pptx', 'xlsx', 'xls', 'ods', 'odt', 'odp', 'pdf', 'rtf'];

export const codes: ErrorCode[] = ['unsupported', 'encrypted', 'too-large', 'malformed', 'timeout', 'unavailable'];

export const describe = (result: Converted): string => `${result.format}: ${result.markdown.length}`;

export const limits: Partial<Limits> = { maxMarkdownBytes: 1024, maxTableCells: 10 };

export async function useConverter(converter: Converter, input: Uint8Array): Promise<string> {
  const result = await converter.convert(input, limits);
  await converter.close();
  return describe(result);
}
