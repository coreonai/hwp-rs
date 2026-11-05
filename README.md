# hwp-rs

Rust parser for Hancom Office HWP5 (`.hwp`) and HWPX (`.hwpx`) documents.

Parses both the legacy OLE2-based binary format and the modern ZIP/XML-based format into a unified `HwpDocument` model.

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
hwp-rs = { git = "https://github.com/coreonai/hwp-rs" }
```

```rust
use hwp_rs::parse;

let doc = parse("document.hwp")?;

for section in &doc.sections {
    for para in &section.paragraphs {
        println!("{}", para.text);
    }
}
```

Format is auto-detected from magic bytes. You can also call `parse_hwp5()` or `parse_hwpx()` directly with any `Read + Seek` reader.

## Document Model

```
HwpDocument
├── header: FileHeader        (version, compression/encryption flags)
├── doc_info: DocInfo
│   ├── char_shapes[]         (font, size, color, bold, italic, underline, ...)
│   ├── para_shapes[]         (alignment, spacing, margins, indent, ...)
│   ├── fonts[7][]            (per script: Korean, English, Hanja, Japanese, Other, Symbol, User)
│   ├── styles[]              (named styles referencing char/para shapes)
│   ├── border_fills[]
│   └── bin_data_defs[]
├── sections[]
│   ├── sec_def               (page dimensions, margins, columns)
│   └── paragraphs[]
│       ├── text: String
│       ├── char_runs[]       (position → char_shape_id mapping)
│       ├── para_shape_id
│       ├── style_id
│       └── controls[]        (Table, Picture, Equation, Field, Bookmark, ...)
└── bin_data[]                (embedded images, OLE objects)
```

## Supported Formats

### HWP5 Binary (`.hwp`)

- OLE2/Compound File Binary container (via `cfb` crate)
- Raw deflate decompression (wbits=-15) with CRC32 trailer verification
- Record header bitfield decoding (tag/level/size, extended size)
- DocInfo stream: font faces (7 script types), char shapes, para shapes, styles, border fills
- BodyText sections: paragraphs, UTF-16LE text with special character handling, char shape runs, tables/cells, inline controls
- BinData extraction (embedded images)

### HWPX XML (`.hwpx`)

- ZIP container with OPF manifest resolution
- Namespaced HWPML XML parsing (`hp:p`, `hp:t`, `hp:tbl`, `hs:sec`, ...)
- Section/paragraph/table/cell extraction
- BinData extraction from ZIP entries

## Example

Dump document structure:

```
cargo run --example dump -- document.hwp
```

```
Version: 5.0.5.0
Flags: compressed=true, encrypted=false, distributed=false

--- DocInfo ---
Char shapes: 48
Para shapes: 36
Styles: 19
Fonts (Korean): 맑은 고딕, 한컴 윤고딕 230, 함초롬돋움, 함초롬바탕, 휴먼명조

--- Sections ---
Section 0: 76 paragraphs, page=59528x84188 (portrait)
  P0: [shape=0, style=0] "<서식 1-2>"
  P3: [shape=0, style=0] "2027학년도 부산국제중학교 신입생 입학 지원 자격 확인서"
  ...
```

## Limitations

- Encrypted documents are rejected (returns `HwpError::Encrypted`)
- Distributed/DRM documents are not supported
- Drawing shapes (line, rect, ellipse, etc.) are modeled but not fully parsed from HWP5 records
- Equation content is not extracted
- HWPX DocInfo (char shapes, para shapes) requires a separate header XML which some files omit

## License

MIT
