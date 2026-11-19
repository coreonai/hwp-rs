//! HWPX writer — serialize [`HwpDocument`] to ZIP/XML HWPX format.

use std::io::{Seek, Write};

use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Writer;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::model::charshape::{CharShape, StrikeoutType, UnderlineType};
use crate::model::control::Control;
use crate::model::parashape::{Alignment, SpacingType};
use crate::model::shape::ShapeObject;
use crate::model::table::{Cell, Table};
use crate::model::{DocInfo, HwpDocument, Paragraph, Section};

const NS_HH: &str = "http://www.hancom.co.kr/hwpml/2011/head";
const NS_HP: &str = "http://www.hancom.co.kr/hwpml/2011/paragraph";
const NS_HS: &str = "http://www.hancom.co.kr/hwpml/2011/section";
const NS_HC: &str = "http://www.hancom.co.kr/hwpml/2011/core";
const NS_HA: &str = "http://www.hancom.co.kr/hwpml/2011/app";
const NS_OPF: &str = "http://www.idpf.org/2007/opf/";

/// Write an [`HwpDocument`] as an HWPX ZIP archive.
pub fn write_hwpx<W: Write + Seek>(doc: &HwpDocument, writer: W) -> crate::Result<()> {
    let mut zip = ZipWriter::new(writer);

    // 1. mimetype (uncompressed)
    zip.start_file("mimetype", SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored))?;
    zip.write_all(b"application/hwp+zip")?;

    let deflated = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // 2. version.xml
    zip.start_file("version.xml", deflated)?;
    write_version_xml(&mut zip)?;

    // 3. META-INF/container.xml
    zip.add_directory("META-INF", deflated)?;
    zip.start_file("META-INF/container.xml", deflated)?;
    write_container_xml(&mut zip)?;

    // 4. Contents/content.hpf
    zip.add_directory("Contents", deflated)?;
    zip.start_file("Contents/content.hpf", deflated)?;
    write_content_hpf(&mut zip, doc)?;

    // 5. Contents/header.xml
    zip.start_file("Contents/header.xml", deflated)?;
    write_header_xml(&mut zip, &doc.doc_info)?;

    // 6. Contents/section*.xml
    for (i, section) in doc.sections.iter().enumerate() {
        zip.start_file(format!("Contents/section{}.xml", i), deflated)?;
        write_section_xml(&mut zip, section, &doc.doc_info, i == 0)?;
    }

    // 7. BinData/*
    if !doc.bin_data.is_empty() {
        zip.add_directory("BinData", deflated)?;
        for entry in &doc.bin_data {
            zip.start_file(format!("BinData/{}", entry.name), deflated)?;
            zip.write_all(&entry.data)?;
        }
    }

    zip.finish()?;
    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────────

fn xml_header<W: Write>(w: &mut W) -> crate::Result<()> {
    let mut xw = Writer::new(w);
    xw.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), Some("yes"))))?;
    Ok(())
}

fn write_version_xml<W: Write>(w: &mut W) -> crate::Result<()> {
    xml_header(w)?;
    w.write_all(b"\n<hv:HCFVersion xmlns:hv=\"urn:hancom:hwpml:2011:version\" Major=\"1\" Minor=\"5\" Micro=\"0\" BuildNumber=\"0\"/>")?;
    Ok(())
}

fn write_container_xml<W: Write>(w: &mut W) -> crate::Result<()> {
    xml_header(w)?;
    w.write_all(br#"
<ocf:container xmlns:ocf="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0">
  <ocf:rootfiles>
    <ocf:rootfile full-path="Contents/content.hpf" media-type="application/hwp+zip"/>
  </ocf:rootfiles>
</ocf:container>"#)?;
    Ok(())
}

fn write_content_hpf<W: Write>(w: &mut W, doc: &HwpDocument) -> crate::Result<()> {
    xml_header(w)?;
    let mut xw = Writer::new_with_indent(w, b' ', 2);

    let mut pkg = BytesStart::new("opf:package");
    pkg.push_attribute(("xmlns:opf", NS_OPF));
    pkg.push_attribute(("version", "1.5"));
    pkg.push_attribute(("id", "hwpx-package"));
    xw.write_event(Event::Start(pkg))?;

    // metadata
    xw.write_event(Event::Start(BytesStart::new("opf:metadata")))?;
    xw.write_event(Event::Start(BytesStart::new("opf:title")))?;
    xw.write_event(Event::Text(BytesText::new("Untitled")))?;
    xw.write_event(Event::End(BytesEnd::new("opf:title")))?;
    xw.write_event(Event::Start(BytesStart::new("opf:language")))?;
    xw.write_event(Event::Text(BytesText::new("ko")))?;
    xw.write_event(Event::End(BytesEnd::new("opf:language")))?;
    xw.write_event(Event::End(BytesEnd::new("opf:metadata")))?;

    // manifest
    xw.write_event(Event::Start(BytesStart::new("opf:manifest")))?;
    write_manifest_item(&mut xw, "header", "Contents/header.xml")?;
    for i in 0..doc.sections.len() {
        write_manifest_item(&mut xw, &format!("section{}", i), &format!("Contents/section{}.xml", i))?;
    }
    xw.write_event(Event::End(BytesEnd::new("opf:manifest")))?;

    // spine
    xw.write_event(Event::Start(BytesStart::new("opf:spine")))?;
    let mut header_ref = BytesStart::new("opf:itemref");
    header_ref.push_attribute(("idref", "header"));
    header_ref.push_attribute(("linear", "yes"));
    xw.write_event(Event::Empty(header_ref))?;
    for i in 0..doc.sections.len() {
        let mut sec_ref = BytesStart::new("opf:itemref");
        sec_ref.push_attribute(("idref", &*format!("section{}", i)));
        sec_ref.push_attribute(("linear", "yes"));
        xw.write_event(Event::Empty(sec_ref))?;
    }
    xw.write_event(Event::End(BytesEnd::new("opf:spine")))?;

    xw.write_event(Event::End(BytesEnd::new("opf:package")))?;
    Ok(())
}

fn write_manifest_item<W: Write>(xw: &mut Writer<W>, id: &str, href: &str) -> crate::Result<()> {
    let mut item = BytesStart::new("opf:item");
    item.push_attribute(("id", id));
    item.push_attribute(("href", href));
    item.push_attribute(("media-type", "application/xml"));
    xw.write_event(Event::Empty(item))?;
    Ok(())
}

// ── header.xml ───────────────────────────────────────────────────────

fn write_header_xml<W: Write>(w: &mut W, di: &DocInfo) -> crate::Result<()> {
    xml_header(w)?;
    let mut xw = Writer::new_with_indent(w, b' ', 2);

    let mut head = BytesStart::new("hh:head");
    head.push_attribute(("xmlns:hh", NS_HH));
    head.push_attribute(("xmlns:hc", NS_HC));
    head.push_attribute(("version", "1.5"));
    head.push_attribute(("secCnt", "1"));
    xw.write_event(Event::Start(head))?;

    // beginNum
    let mut bn = BytesStart::new("hh:beginNum");
    bn.push_attribute(("page", "1"));
    bn.push_attribute(("footnote", "1"));
    bn.push_attribute(("endnote", "1"));
    bn.push_attribute(("pic", "1"));
    bn.push_attribute(("tbl", "1"));
    bn.push_attribute(("equation", "1"));
    xw.write_event(Event::Empty(bn))?;

    xw.write_event(Event::Start(BytesStart::new("hh:refList")))?;

    // fontfaces
    write_fontfaces(&mut xw, di)?;

    // borderFills
    write_border_fills(&mut xw, di)?;

    // charShapes
    write_char_shapes(&mut xw, di)?;

    // paraShapes
    write_para_shapes(&mut xw, di)?;

    // styles
    write_styles(&mut xw, di)?;

    xw.write_event(Event::End(BytesEnd::new("hh:refList")))?;
    xw.write_event(Event::End(BytesEnd::new("hh:head")))?;
    Ok(())
}

const SCRIPT_NAMES: [&str; 7] = ["HANGUL", "LATIN", "HANJA", "JAPANESE", "OTHER", "SYMBOL", "USER"];

fn write_fontfaces<W: Write>(xw: &mut Writer<W>, di: &DocInfo) -> crate::Result<()> {
    let mut ff = BytesStart::new("hh:fontfaces");
    ff.push_attribute(("itemCnt", "7"));
    xw.write_event(Event::Start(ff))?;

    for (si, script) in SCRIPT_NAMES.iter().enumerate() {
        let fonts = di.fonts.get(si);
        let count = fonts.map(|f| f.len()).unwrap_or(0);

        let mut face = BytesStart::new("hh:fontface");
        face.push_attribute(("lang", *script));
        face.push_attribute(("fontCnt", &*count.to_string()));

        if count == 0 {
            xw.write_event(Event::Empty(face))?;
        } else {
            xw.write_event(Event::Start(face))?;
            for (fi, font) in fonts.unwrap().iter().enumerate() {
                let mut f = BytesStart::new("hh:font");
                f.push_attribute(("id", &*fi.to_string()));
                f.push_attribute(("face", font.name.as_str()));
                f.push_attribute(("type", "TTF"));
                f.push_attribute(("isEmbedded", "0"));
                xw.write_event(Event::Empty(f))?;
            }
            xw.write_event(Event::End(BytesEnd::new("hh:fontface")))?;
        }
    }

    xw.write_event(Event::End(BytesEnd::new("hh:fontfaces")))?;
    Ok(())
}

fn write_border_fills<W: Write>(xw: &mut Writer<W>, di: &DocInfo) -> crate::Result<()> {
    let mut bf = BytesStart::new("hh:borderFills");
    bf.push_attribute(("itemCnt", &*di.border_fills.len().to_string()));
    xw.write_event(Event::Start(bf))?;

    for (i, fill) in di.border_fills.iter().enumerate() {
        let mut el = BytesStart::new("hh:borderFill");
        el.push_attribute(("id", &*(i + 1).to_string()));
        el.push_attribute(("threeD", "0"));
        el.push_attribute(("shadow", "0"));
        el.push_attribute(("centerLine", "NONE"));
        el.push_attribute(("breakCellSeparateLine", "0"));
        xw.write_event(Event::Start(el))?;

        // borders
        for (name, btype, color) in [
            ("hh:leftBorder", fill.border_type_left, fill.border_color_left),
            ("hh:rightBorder", fill.border_type_right, fill.border_color_right),
            ("hh:topBorder", fill.border_type_top, fill.border_color_top),
            ("hh:bottomBorder", fill.border_type_bottom, fill.border_color_bottom),
        ] {
            let mut b = BytesStart::new(name);
            b.push_attribute(("type", border_type_str(btype)));
            b.push_attribute(("width", "0.1 mm"));
            b.push_attribute(("color", &*format!("#{:06X}", color)));
            xw.write_event(Event::Empty(b))?;
        }

        // fill
        if let Some(fc) = fill.fill_color {
            xw.write_event(Event::Start(BytesStart::new("hc:fillBrush")))?;
            let mut wb = BytesStart::new("hc:winBrush");
            wb.push_attribute(("faceColor", &*format!("#{:06X}", fc)));
            wb.push_attribute(("hatchColor", "#000000"));
            wb.push_attribute(("alpha", "0"));
            xw.write_event(Event::Empty(wb))?;
            xw.write_event(Event::End(BytesEnd::new("hc:fillBrush")))?;
        }

        xw.write_event(Event::End(BytesEnd::new("hh:borderFill")))?;
    }

    xw.write_event(Event::End(BytesEnd::new("hh:borderFills")))?;
    Ok(())
}

fn border_type_str(t: u8) -> &'static str {
    match t {
        0 => "NONE",
        1 => "SOLID",
        2 => "DASH",
        3 => "DOT",
        4 => "DASH_DOT",
        5 => "DASH_DOT_DOT",
        6 => "LONG_DASH",
        7 => "CIRCLE",
        8 => "DOUBLE_SLIM",
        _ => "NONE",
    }
}

fn write_char_shapes<W: Write>(xw: &mut Writer<W>, di: &DocInfo) -> crate::Result<()> {
    let mut cs_el = BytesStart::new("hh:charShapes");
    cs_el.push_attribute(("itemCnt", &*di.char_shapes.len().to_string()));
    xw.write_event(Event::Start(cs_el))?;

    for (i, cs) in di.char_shapes.iter().enumerate() {
        let mut el = BytesStart::new("hh:charShape");
        el.push_attribute(("id", &*i.to_string()));
        el.push_attribute(("height", &*cs.height.to_string()));
        el.push_attribute(("textColor", &*format!("{:06X}", cs.text_color)));
        el.push_attribute(("shadeColor", &*format!("{:06X}", cs.shade_color)));
        el.push_attribute(("useFontSpace", "0"));
        el.push_attribute(("useKerning", "0"));
        el.push_attribute(("symMark", "0"));
        el.push_attribute(("borderFillIDRef", &*cs.border_fill_id.to_string()));

        write_char_shape_attrs(&mut el, cs);
        xw.write_event(Event::Start(el))?;

        // fontRef per script
        for (si, script) in SCRIPT_NAMES.iter().enumerate() {
            let mut fr = BytesStart::new("hh:fontRef");
            fr.push_attribute(("script", *script));
            fr.push_attribute(("fontId", &*cs.font_ids[si].to_string()));
            fr.push_attribute(("ratio", &*cs.font_ratios[si].to_string()));
            fr.push_attribute(("spacing", &*cs.font_spacings[si].to_string()));
            fr.push_attribute(("relSize", &*cs.font_rel_sizes[si].to_string()));
            fr.push_attribute(("offset", &*cs.font_offsets[si].to_string()));
            xw.write_event(Event::Empty(fr))?;
        }

        xw.write_event(Event::End(BytesEnd::new("hh:charShape")))?;
    }

    xw.write_event(Event::End(BytesEnd::new("hh:charShapes")))?;
    Ok(())
}

fn write_char_shape_attrs(el: &mut BytesStart, cs: &CharShape) {
    if cs.bold {
        el.push_attribute(("bold", "1"));
    }
    if cs.italic {
        el.push_attribute(("italic", "1"));
    }
    let ul = match cs.underline {
        UnderlineType::None => 0u8,
        UnderlineType::Single => 1,
        UnderlineType::Double => 2,
        UnderlineType::Thick => 3,
        UnderlineType::Other(n) => n,
        _ => 0,
    };
    if ul > 0 {
        el.push_attribute(("underlineType", &*ul.to_string()));
    }
    let st = match cs.strikeout {
        StrikeoutType::None => 0u8,
        StrikeoutType::Single => 1,
        StrikeoutType::Other(n) => n,
    };
    if st > 0 {
        el.push_attribute(("strikeoutType", &*st.to_string()));
    }
    if cs.superscript {
        el.push_attribute(("superscript", "1"));
    }
    if cs.subscript {
        el.push_attribute(("subscript", "1"));
    }
}

fn write_para_shapes<W: Write>(xw: &mut Writer<W>, di: &DocInfo) -> crate::Result<()> {
    let mut ps_el = BytesStart::new("hh:paraShapes");
    ps_el.push_attribute(("itemCnt", &*di.para_shapes.len().to_string()));
    xw.write_event(Event::Start(ps_el))?;

    for (i, ps) in di.para_shapes.iter().enumerate() {
        let mut el = BytesStart::new("hh:paraShape");
        el.push_attribute(("id", &*i.to_string()));
        el.push_attribute(("align", align_str(ps.align)));
        el.push_attribute(("lineSpacing", &*ps.line_spacing.to_string()));
        el.push_attribute(("lineSpacingType", spacing_type_str(ps.line_spacing_type)));
        el.push_attribute(("spaceBefore", &*ps.space_before.to_string()));
        el.push_attribute(("spaceAfter", &*ps.space_after.to_string()));
        el.push_attribute(("marginLeft", &*ps.margin_left.to_string()));
        el.push_attribute(("marginRight", &*ps.margin_right.to_string()));
        el.push_attribute(("indent", &*ps.indent.to_string()));
        el.push_attribute(("tabDefId", &*ps.tab_def_id.to_string()));
        el.push_attribute(("borderFillId", &*ps.border_fill_id.to_string()));
        el.push_attribute(("headingType", &*ps.heading_type.to_string()));
        el.push_attribute(("headingLevel", &*ps.heading_level.to_string()));
        xw.write_event(Event::Empty(el))?;
    }

    xw.write_event(Event::End(BytesEnd::new("hh:paraShapes")))?;
    Ok(())
}

fn align_str(a: Alignment) -> &'static str {
    match a {
        Alignment::Justify => "Justify",
        Alignment::Left => "Left",
        Alignment::Right => "Right",
        Alignment::Center => "Center",
        Alignment::Distribute => "Distribute",
        Alignment::DistributeSpace => "Distribute",
        Alignment::Other(_) => "Left",
    }
}

fn spacing_type_str(st: SpacingType) -> &'static str {
    match st {
        SpacingType::Percent => "Percent",
        SpacingType::Fixed => "Fixed",
        SpacingType::BetweenLines => "BetweenLines",
        SpacingType::AtLeast => "AtLeast",
    }
}

fn write_styles<W: Write>(xw: &mut Writer<W>, di: &DocInfo) -> crate::Result<()> {
    let mut s_el = BytesStart::new("hh:styles");
    s_el.push_attribute(("itemCnt", &*di.styles.len().to_string()));
    xw.write_event(Event::Start(s_el))?;

    for (i, style) in di.styles.iter().enumerate() {
        let mut el = BytesStart::new("hh:style");
        el.push_attribute(("id", &*i.to_string()));
        el.push_attribute(("name", style.name.as_str()));
        el.push_attribute(("engName", style.english_name.as_str()));
        el.push_attribute(("type", &*style.style_type.to_string()));
        el.push_attribute(("nextStyleId", &*style.next_style_id.to_string()));
        el.push_attribute(("charShapeId", &*style.char_shape_id.to_string()));
        el.push_attribute(("paraShapeId", &*style.para_shape_id.to_string()));
        xw.write_event(Event::Empty(el))?;
    }

    xw.write_event(Event::End(BytesEnd::new("hh:styles")))?;
    Ok(())
}

// ── section*.xml ─────────────────────────────────────────────────────

fn write_section_xml<W: Write>(
    w: &mut W,
    section: &Section,
    di: &DocInfo,
    is_first: bool,
) -> crate::Result<()> {
    xml_header(w)?;
    let mut xw = Writer::new_with_indent(w, b' ', 2);

    let mut sec = BytesStart::new("hs:sec");
    sec.push_attribute(("xmlns:hs", NS_HS));
    sec.push_attribute(("xmlns:hp", NS_HP));
    sec.push_attribute(("xmlns:ha", NS_HA));
    sec.push_attribute(("xmlns:hc", NS_HC));
    xw.write_event(Event::Start(sec))?;

    for (pi, para) in section.paragraphs.iter().enumerate() {
        let include_sec_pr = is_first && pi == 0;
        write_paragraph(&mut xw, para, di, section, include_sec_pr)?;
    }

    xw.write_event(Event::End(BytesEnd::new("hs:sec")))?;
    Ok(())
}

fn write_paragraph<W: Write>(
    xw: &mut Writer<W>,
    para: &Paragraph,
    di: &DocInfo,
    section: &Section,
    include_sec_pr: bool,
) -> crate::Result<()> {
    let mut p = BytesStart::new("hp:p");
    p.push_attribute(("paraPrIDRef", &*para.para_shape_id.to_string()));
    p.push_attribute(("styleIDRef", &*para.style_id.to_string()));
    p.push_attribute(("pageBreak", "0"));
    p.push_attribute(("columnBreak", "0"));
    p.push_attribute(("merged", "0"));
    xw.write_event(Event::Start(p))?;

    // Write text runs
    let chars: Vec<char> = para.text.chars().collect();

    if para.char_runs.is_empty() && !chars.is_empty() {
        // Single run for entire text
        xw.write_event(Event::Start(BytesStart::new("hp:run")))?;
        if include_sec_pr {
            write_sec_pr(xw, section)?;
        }
        xw.write_event(Event::Start(BytesStart::new("hp:t")))?;
        xw.write_event(Event::Text(BytesText::new(&para.text)))?;
        xw.write_event(Event::End(BytesEnd::new("hp:t")))?;
        xw.write_event(Event::End(BytesEnd::new("hp:run")))?;
    } else {
        for (ri, run) in para.char_runs.iter().enumerate() {
            let start = run.start_pos as usize;
            let end = if ri + 1 < para.char_runs.len() {
                (para.char_runs[ri + 1].start_pos as usize).min(chars.len())
            } else {
                chars.len()
            };
            if start >= chars.len() {
                break;
            }
            let end = end.min(chars.len());
            let text: String = chars[start..end].iter().collect();

            let mut run_el = BytesStart::new("hp:run");
            run_el.push_attribute(("charPrIDRef", &*run.char_shape_id.to_string()));
            xw.write_event(Event::Start(run_el))?;

            if include_sec_pr && ri == 0 {
                write_sec_pr(xw, section)?;
            }

            if !text.is_empty() {
                xw.write_event(Event::Start(BytesStart::new("hp:t")))?;
                xw.write_event(Event::Text(BytesText::new(&text)))?;
                xw.write_event(Event::End(BytesEnd::new("hp:t")))?;
            }

            xw.write_event(Event::End(BytesEnd::new("hp:run")))?;
        }
    }

    // Write controls
    for ctrl in &para.controls {
        match ctrl {
            Control::Table(table) => write_table(xw, table, di)?,
            Control::Picture(shape) => write_picture(xw, shape)?,
            Control::Equation(eq) => write_equation(xw, eq)?,
            _ => {}
        }
    }

    xw.write_event(Event::End(BytesEnd::new("hp:p")))?;
    Ok(())
}

fn write_sec_pr<W: Write>(xw: &mut Writer<W>, section: &Section) -> crate::Result<()> {
    let pd = &section.sec_def.page_def;

    let mut sp = BytesStart::new("hp:secPr");
    sp.push_attribute(("textDirection", "HORIZONTAL"));
    sp.push_attribute(("spaceColumns", &*section.sec_def.column_spacing.to_string()));
    xw.write_event(Event::Start(sp))?;

    let mut pp = BytesStart::new("hp:pagePr");
    pp.push_attribute(("landscape", if pd.landscape { "WIDELY" } else { "PORTRAIT" }));
    pp.push_attribute(("width", &*pd.width.to_string()));
    pp.push_attribute(("height", &*pd.height.to_string()));
    pp.push_attribute(("gutterType", "LEFT_ONLY"));
    xw.write_event(Event::Start(pp))?;

    let mut margin = BytesStart::new("hp:margin");
    margin.push_attribute(("header", &*pd.margin_header.to_string()));
    margin.push_attribute(("footer", &*pd.margin_footer.to_string()));
    margin.push_attribute(("gutter", &*pd.margin_gutter.to_string()));
    margin.push_attribute(("left", &*pd.margin_left.to_string()));
    margin.push_attribute(("right", &*pd.margin_right.to_string()));
    margin.push_attribute(("top", &*pd.margin_top.to_string()));
    margin.push_attribute(("bottom", &*pd.margin_bottom.to_string()));
    xw.write_event(Event::Empty(margin))?;

    xw.write_event(Event::End(BytesEnd::new("hp:pagePr")))?;
    xw.write_event(Event::End(BytesEnd::new("hp:secPr")))?;
    Ok(())
}

fn write_table<W: Write>(xw: &mut Writer<W>, table: &Table, di: &DocInfo) -> crate::Result<()> {
    // Wrap table in a run
    xw.write_event(Event::Start(BytesStart::new("hp:run")))?;

    let mut tbl = BytesStart::new("hp:tbl");
    tbl.push_attribute(("rowCnt", &*table.rows.to_string()));
    tbl.push_attribute(("colCnt", &*table.cols.to_string()));
    tbl.push_attribute(("cellSpacing", &*table.cell_spacing.to_string()));
    tbl.push_attribute(("borderFillIDRef", &*table.border_fill_id.to_string()));
    xw.write_event(Event::Start(tbl))?;

    // Group cells by row
    let rows = table.rows as usize;
    for r in 0..rows {
        xw.write_event(Event::Start(BytesStart::new("hp:tr")))?;

        let row_cells: Vec<&Cell> = table
            .cells
            .iter()
            .filter(|c| c.row_addr as usize == r)
            .collect();

        for cell in &row_cells {
            write_table_cell(xw, cell, di)?;
        }

        xw.write_event(Event::End(BytesEnd::new("hp:tr")))?;
    }

    xw.write_event(Event::End(BytesEnd::new("hp:tbl")))?;
    xw.write_event(Event::End(BytesEnd::new("hp:run")))?;
    Ok(())
}

fn write_table_cell<W: Write>(
    xw: &mut Writer<W>,
    cell: &Cell,
    di: &DocInfo,
) -> crate::Result<()> {
    let mut tc = BytesStart::new("hp:tc");
    tc.push_attribute(("borderFillIDRef", &*cell.border_fill_id.to_string()));
    xw.write_event(Event::Start(tc))?;

    // subList with cell paragraphs
    let mut sl = BytesStart::new("hp:subList");
    sl.push_attribute(("textDirection", "HORIZONTAL"));
    sl.push_attribute(("vertAlign", "CENTER"));
    xw.write_event(Event::Start(sl))?;

    for para in &cell.paragraphs {
        write_paragraph(xw, para, di, &Section::default(), false)?;
    }

    xw.write_event(Event::End(BytesEnd::new("hp:subList")))?;

    // cellAddr
    let mut addr = BytesStart::new("hp:cellAddr");
    addr.push_attribute(("colAddr", &*cell.col_addr.to_string()));
    addr.push_attribute(("rowAddr", &*cell.row_addr.to_string()));
    xw.write_event(Event::Empty(addr))?;

    // cellSpan
    let mut span = BytesStart::new("hp:cellSpan");
    span.push_attribute(("colSpan", &*cell.col_span.to_string()));
    span.push_attribute(("rowSpan", &*cell.row_span.to_string()));
    xw.write_event(Event::Empty(span))?;

    // cellSz
    let mut sz = BytesStart::new("hp:cellSz");
    sz.push_attribute(("width", &*cell.width.to_string()));
    sz.push_attribute(("height", &*cell.height.to_string()));
    xw.write_event(Event::Empty(sz))?;

    // cellMargin
    let mut cm = BytesStart::new("hp:cellMargin");
    cm.push_attribute(("left", &*cell.padding[0].to_string()));
    cm.push_attribute(("right", &*cell.padding[1].to_string()));
    cm.push_attribute(("top", &*cell.padding[2].to_string()));
    cm.push_attribute(("bottom", &*cell.padding[3].to_string()));
    xw.write_event(Event::Empty(cm))?;

    xw.write_event(Event::End(BytesEnd::new("hp:tc")))?;
    Ok(())
}

fn write_picture<W: Write>(xw: &mut Writer<W>, shape: &ShapeObject) -> crate::Result<()> {
    if let ShapeObject::Picture {
        bin_data_id,
        width,
        height,
        ..
    } = shape
    {
        xw.write_event(Event::Start(BytesStart::new("hp:run")))?;

        let mut pic = BytesStart::new("hp:pic");
        pic.push_attribute(("textWrap", "TOP_AND_BOTTOM"));
        xw.write_event(Event::Start(pic))?;

        let mut sz = BytesStart::new("hp:sz");
        sz.push_attribute(("width", &*width.to_string()));
        sz.push_attribute(("height", &*height.to_string()));
        xw.write_event(Event::Empty(sz))?;

        let mut bin_ref = BytesStart::new("hp:binItemIDRef");
        bin_ref.push_attribute(("id", &*bin_data_id.to_string()));
        xw.write_event(Event::Empty(bin_ref))?;

        xw.write_event(Event::End(BytesEnd::new("hp:pic")))?;
        xw.write_event(Event::End(BytesEnd::new("hp:run")))?;
    } else if let ShapeObject::Container { children } = shape {
        for child in children {
            write_picture(xw, child)?;
        }
    }
    Ok(())
}

fn write_equation<W: Write>(
    xw: &mut Writer<W>,
    eq: &crate::model::control::Equation,
) -> crate::Result<()> {
    xw.write_event(Event::Start(BytesStart::new("hp:run")))?;

    let mut eqn = BytesStart::new("hp:equation");
    eqn.push_attribute(("fontName", eq.font_name.as_str()));
    eqn.push_attribute(("fontSize", &*eq.font_size.to_string()));
    xw.write_event(Event::Start(eqn))?;

    xw.write_event(Event::Start(BytesStart::new("hp:script")))?;
    xw.write_event(Event::Text(BytesText::new(&eq.script)))?;
    xw.write_event(Event::End(BytesEnd::new("hp:script")))?;

    xw.write_event(Event::End(BytesEnd::new("hp:equation")))?;
    xw.write_event(Event::End(BytesEnd::new("hp:run")))?;
    Ok(())
}
