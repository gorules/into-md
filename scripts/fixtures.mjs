import { writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const { OfficeParser } = require('officeparser');
const { Workbook } = require('exceljs');
const JSZip = require('jszip');

const out = new URL('../test/fixtures/', import.meta.url);

const SOURCE = `# Loan Policy

Applicants must be **18+** and *employed*. See [the guide](https://gorules.io/docs).

## Eligibility

- Income > 30k
- No defaults

1. Submit application
2. Await review

| Grade | Min score | Rate |
| --- | --- | --- |
| A | 750 | 3.5% |
| B | 650 | 5.0% |
`;

const fromMarkdown = async (format) => {
  const ast = await OfficeParser.parseOffice(Buffer.from(SOURCE), { fileType: 'md' });
  const { value } = await ast.to(format);
  return Buffer.from(value);
};

const zip = (files) => {
  const archive = new JSZip();
  for (const [name, content] of Object.entries(files)) {
    archive.file(name, content, name === 'mimetype' ? { compression: 'STORE' } : undefined);
  }
  return archive.generateAsync({ type: 'nodebuffer', compression: 'DEFLATE' });
};

const xlsx = async () => {
  const workbook = new Workbook();
  workbook.addWorksheet('Rates').addRows([
    ['Grade', 'Min', 'Rate', 'Effective'],
    ['A', 750, 0.035, new Date(Date.UTC(2026, 0, 15))],
    ['B', 650, 0.05, new Date(Date.UTC(2026, 1, 1))],
  ]);
  workbook.addWorksheet('Empty');
  workbook.addWorksheet('Limits | caps').addRows([
    ['Country', 'Max'],
    ['US', 100000],
  ]);
  return Buffer.from(await workbook.xlsx.writeBuffer());
};

const PRESENTATION_NS =
  'xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"';

const shape = (placeholder, paragraphs) =>
  `<p:sp><p:nvSpPr><p:cNvPr id="1" name="s"/><p:cNvSpPr/><p:nvPr>${placeholder}</p:nvPr></p:nvSpPr><p:txBody>${paragraphs}</p:txBody></p:sp>`;

const para = (text, attrs = '', runAttrs = '') => `<a:p><a:pPr ${attrs}/><a:r><a:rPr ${runAttrs}/><a:t>${text}</a:t></a:r></a:p>`;

const pptx = () =>
  zip({
    '[Content_Types].xml': `<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/></Types>`,
    '_rels/.rels': `<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/></Relationships>`,
    'ppt/presentation.xml': `<?xml version="1.0"?><p:presentation ${PRESENTATION_NS}><p:sldIdLst><p:sldId id="256" r:id="rId3"/><p:sldId id="257" r:id="rId2"/></p:sldIdLst></p:presentation>`,
    'ppt/_rels/presentation.xml.rels': `<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide2.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/></Relationships>`,
    'ppt/slides/slide1.xml': `<?xml version="1.0"?><p:sld ${PRESENTATION_NS}><p:cSld><p:spTree>${shape('<p:ph type="body" idx="1"/>', para('Income &gt; 30k') + para('Verified by payslip', 'lvl="1"') + para('No defaults', '', 'b="1"'))}${shape('<p:ph type="title"/>', para('Eligibility &amp; scope'))}</p:spTree></p:cSld></p:sld>`,
    'ppt/slides/_rels/slide2.xml.rels': `<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide" Target="../notesSlides/notesSlide4.xml"/><Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://gorules.io/pricing" TargetMode="External"/></Relationships>`,
    'ppt/notesSlides/notesSlide4.xml': `<?xml version="1.0"?><p:notes ${PRESENTATION_NS}><p:cSld><p:spTree>${shape('<p:ph type="sldImg"/>', '')}${shape('<p:ph type="body" idx="1"/>', para('Mention the 2026 rate review.') + para('Keep it short.'))}${shape('<p:ph type="sldNum"/>', para('2'))}</p:spTree></p:cSld></p:notes>`,
    'ppt/slides/slide2.xml': `<?xml version="1.0"?><p:sld ${PRESENTATION_NS}><p:cSld><p:spTree>${shape('<p:ph type="title"/>', para('Rates'))}<p:graphicFrame><a:graphic><a:graphicData><a:tbl><a:tr><a:tc><a:txBody>${para('Grade')}</a:txBody></a:tc><a:tc><a:txBody>${para('Rate')}</a:txBody></a:tc></a:tr><a:tr><a:tc><a:txBody>${para('A')}</a:txBody></a:tc><a:tc><a:txBody>${para('3.5%')}</a:txBody></a:tc></a:tr></a:tbl></a:graphicData></a:graphic></p:graphicFrame>${shape('', `<a:p><a:r><a:t>Footnote with </a:t></a:r><a:r><a:rPr><a:hlinkClick r:id="rId7"/></a:rPr><a:t>pricing link</a:t></a:r></a:p>`)}</p:spTree></p:cSld></p:sld>`,
  });

const ODF_NS =
  'xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:presentation="urn:oasis:names:tc:opendocument:xmlns:presentation:1.0" xmlns:office-value="urn:x"';

const ods = () =>
  zip({
    mimetype: 'application/vnd.oasis.opendocument.spreadsheet',
    'META-INF/manifest.xml': `<?xml version="1.0"?><manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0"><manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.spreadsheet"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/></manifest:manifest>`,
    'content.xml': `<?xml version="1.0"?><office:document-content ${ODF_NS}><office:body><office:spreadsheet><table:table table:name="Rates"><table:table-row><table:table-cell office:value-type="string"><text:p>Grade</text:p></table:table-cell><table:table-cell office:value-type="string"><text:p>Min</text:p></table:table-cell></table:table-row><table:table-row><table:table-cell office:value-type="string"><text:p>A</text:p></table:table-cell><table:table-cell office:value-type="float" office:value="750"><text:p>750</text:p></table:table-cell></table:table-row></table:table></office:spreadsheet></office:body></office:document-content>`,
  });

const odp = () =>
  zip({
    mimetype: 'application/vnd.oasis.opendocument.presentation',
    'content.xml': `<?xml version="1.0"?><office:document-content ${ODF_NS}><office:body><office:presentation><draw:page draw:name="page1"><draw:frame><draw:text-box><text:p>Loan Policy</text:p></draw:text-box></draw:frame><draw:frame><draw:text-box><text:list><text:list-item><text:p>Income above 30k</text:p></text:list-item></text:list></draw:text-box></draw:frame></draw:page><draw:page draw:name="Pricing"><draw:frame><draw:text-box><text:p>Rates<text:s text:c="3"/>vary</text:p></draw:text-box></draw:frame></draw:page></office:presentation></office:body></office:document-content>`,
  });

const pdf = () => {
  const lines = ['Loan Policy', 'Applicants must be 18 or older.', 'Income must exceed 30k.'];
  const stream = `BT /F1 14 Tf 72 720 Td ${lines.map((line, index) => `${index ? '0 -24 Td ' : ''}(${line}) Tj`).join(' ')} ET`;
  const objects = [
    '<< /Type /Catalog /Pages 2 0 R >>',
    '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>',
    `<< /Length ${stream.length} >>\nstream\n${stream}\nendstream`,
    '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>',
  ];
  let body = '%PDF-1.4\n';
  const offsets = objects.map((object, index) => {
    const offset = body.length;
    body += `${index + 1} 0 obj\n${object}\nendobj\n`;
    return offset;
  });
  const xref = body.length;
  body += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n${offsets.map((offset) => `${String(offset).padStart(10, '0')} 00000 n \n`).join('')}`;
  body += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  return Buffer.from(body, 'latin1');
};

const fixtures = {
  'policy.docx': await fromMarkdown('docx'),
  'policy.odt': await fromMarkdown('odt'),
  'policy.rtf': await fromMarkdown('rtf'),
  'rates.xlsx': await xlsx(),
  'rates.ods': await ods(),
  'deck.pptx': await pptx(),
  'deck.odp': await odp(),
  'policy.pdf': pdf(),
};

for (const [name, bytes] of Object.entries(fixtures)) {
  await writeFile(new URL(name, out), bytes);
  console.log(name, bytes.length);
}
