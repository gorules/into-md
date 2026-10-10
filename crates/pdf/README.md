# into-md-pdf

PDF text extraction used by into-md. A fork of [pdf-extract](https://github.com/jrmuizel/pdf-extract) 0.12.1 by Jeff Muizelaar (MIT, see LICENSE), reduced for size:

- CMap and Type1 encoding parsing are hand-written (`src/ps.rs`) instead of the pom-based adobe-cmap-parser and type1-encoding-parser crates; codespace widths and CID ranges are read correctly and malformed input no longer panics.
- Standard 14 font metrics store resolved code points instead of glyph names.
- The glyph name table (`src/glyphnames.rs`) is generated from the Adobe Glyph List and its ZapfDingbats list ([adobe-type-tools/agl-aglfn](https://github.com/adobe-type-tools/agl-aglfn) at 4036a9ca80a62f64f9de4f7321a9a045ad0ecfd6, BSD-3-Clause, see LICENSE-AGL.md), plus the AGL `uniXXXX`/`uXXXX` naming rules and the f-ligatures.
- unicode-normalization and logging are compiled out.
