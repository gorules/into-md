export const FORMATS = [undefined, 'docx', 'pptx', 'xlsx', 'xls', 'ods', 'odt', 'odp', 'pdf', 'rtf'];

const ERROR_CODES = { 1: 'unsupported', 2: 'too-large', 3: 'malformed', 4: 'encrypted' };

export const DEFAULT_LIMITS = Object.freeze({
  maxInputBytes: 25 * 1024 * 1024,
  maxUncompressedBytes: 20 * 1024 * 1024,
  maxZipEntries: 2000,
  maxXmlElements: 500_000,
  maxTableCells: 200_000,
  maxMarkdownBytes: 2 * 1024 * 1024,
});

export class IntoMdError extends Error {
  constructor(code, message) {
    super(message);
    this.name = 'IntoMdError';
    this.code = code;
  }
}

const PDF_MAGIC = [0x25, 0x50, 0x44, 0x46, 0x2d];
const CFB_MAGIC = [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];

const startsWith = (bytes, magic) => bytes.length >= magic.length && magic.every((byte, index) => bytes[index] === byte);

export const needsFullModule = (bytes) => startsWith(bytes, PDF_MAGIC) || startsWith(bytes, CFB_MAGIC);

export const toBytes = (input) => {
  if (input instanceof Uint8Array) return input;
  if (input instanceof ArrayBuffer) return new Uint8Array(input);
  if (ArrayBuffer.isView(input)) return new Uint8Array(input.buffer, input.byteOffset, input.byteLength);
  throw new TypeError('input must be a Uint8Array, Buffer or ArrayBuffer');
};

export const resolveLimits = (limits = {}) => ({ ...DEFAULT_LIMITS, ...limits });

export const checkInputSize = (bytes, limits) => {
  if (bytes.byteLength > resolveLimits(limits).maxInputBytes) {
    throw new IntoMdError('too-large', 'input limit exceeded');
  }
};

const NAMED_CODE_PAGES = {
  437: 'ibm866',
  866: 'ibm866',
  874: 'windows-874',
  932: 'shift_jis',
  936: 'gbk',
  949: 'euc-kr',
  950: 'big5',
  10000: 'macintosh',
  10007: 'x-mac-cyrillic',
  20866: 'koi8-r',
  21866: 'koi8-u',
  20932: 'euc-jp',
  51932: 'euc-jp',
  50220: 'iso-2022-jp',
  50221: 'iso-2022-jp',
  50222: 'iso-2022-jp',
  51949: 'euc-kr',
  54936: 'gb18030',
};

const codePageLabel = (codePage) => {
  if (NAMED_CODE_PAGES[codePage]) return NAMED_CODE_PAGES[codePage];
  if (codePage >= 1250 && codePage <= 1258) return `windows-${codePage}`;
  if (codePage >= 28592 && codePage <= 28606) return `iso-8859-${codePage - 28590}`;
  return undefined;
};

const decoders = new Map();
const encoder = new TextEncoder();

const decoderFor = (codePage) => {
  if (!decoders.has(codePage)) {
    const label = codePageLabel(codePage);
    let decoder = null;
    try {
      decoder = label ? new TextDecoder(label) : null;
    } catch {
      decoder = null;
    }
    decoders.set(codePage, decoder);
  }
  return decoders.get(codePage);
};

export function convertWithModule(module, bytes, limits) {
  const options = resolveLimits(limits);
  let memory;
  const imports = {
    into_md: {
      decode(codePage, source, sourceLength, target, targetCapacity) {
        const decoder = decoderFor(codePage);
        if (!decoder) return -1;
        const text = decoder.decode(new Uint8Array(memory.buffer, source, sourceLength));
        return encoder.encodeInto(text, new Uint8Array(memory.buffer, target, targetCapacity)).written;
      },
    },
  };

  const { exports } = new WebAssembly.Instance(module, imports);
  memory = exports.memory;
  let status;
  try {
    const pointer = exports.into_md_alloc(bytes.byteLength);
    new Uint8Array(memory.buffer, pointer, bytes.byteLength).set(bytes);
    status = exports.into_md_convert(
      pointer,
      bytes.byteLength,
      options.maxUncompressedBytes,
      options.maxZipEntries,
      options.maxXmlElements,
      options.maxTableCells,
      options.maxMarkdownBytes
    );
  } catch (error) {
    throw new IntoMdError('malformed', `conversion aborted: ${error.message}`);
  }

  const output = new TextDecoder().decode(new Uint8Array(memory.buffer, exports.into_md_output_ptr(), exports.into_md_output_len()));
  if (status !== 0) {
    throw new IntoMdError(ERROR_CODES[status] ?? 'malformed', output);
  }
  return { format: FORMATS[exports.into_md_format()], markdown: output };
}

export const serializeError = (error) => ({
  code: error instanceof IntoMdError ? error.code : 'malformed',
  message: error.message,
});
