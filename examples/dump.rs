//! Dump the structure of an HWP or HWPX document.
//!
//! Usage: cargo run --example dump -- <file.hwp|file.hwpx>

use hwp_rs::model::control::Control;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <file.hwp|file.hwpx>", args[0]);
        std::process::exit(1);
    }

    let path = &args[1];
    println!("Parsing: {path}");

    match hwp_rs::parse(path) {
        Ok(doc) => {
            let h = &doc.header;
            println!(
                "Version: {}.{}.{}.{}",
                h.version.0, h.version.1, h.version.2, h.version.3
            );
            println!(
                "Flags: compressed={}, encrypted={}, distributed={}",
                h.compressed, h.encrypted, h.distributed
            );

            println!("\n--- DocInfo ---");
            println!("Char shapes: {}", doc.doc_info.char_shapes.len());
            println!("Para shapes: {}", doc.doc_info.para_shapes.len());
            println!("Styles: {}", doc.doc_info.styles.len());
            for (i, fonts) in doc.doc_info.fonts.iter().enumerate() {
                let script = [
                    "Korean", "English", "Hanja", "Japanese", "Other", "Symbol", "User",
                ][i];
                if !fonts.is_empty() {
                    println!(
                        "Fonts ({}): {}",
                        script,
                        fonts
                            .iter()
                            .map(|f| f.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
            }

            println!("\n--- Sections ---");
            for (si, section) in doc.sections.iter().enumerate() {
                let pd = &section.sec_def.page_def;
                println!(
                    "Section {}: {} paragraphs, page={}x{} ({})",
                    si,
                    section.paragraphs.len(),
                    pd.width,
                    pd.height,
                    if pd.landscape { "landscape" } else { "portrait" }
                );

                for (pi, para) in section.paragraphs.iter().enumerate() {
                    let text_preview: String = para.text.chars().take(80).collect();
                    let text_preview = text_preview.replace('\n', "\\n").replace('\t', "\\t");
                    println!(
                        "  P{}: [shape={}, style={}] {:?}",
                        pi, para.para_shape_id, para.style_id, text_preview
                    );

                    if !para.char_runs.is_empty() {
                        print!("    Char runs:");
                        for run in &para.char_runs {
                            print!(" @{}→cs{}", run.start_pos, run.char_shape_id);
                        }
                        println!();
                    }

                    for ctrl in &para.controls {
                        match ctrl {
                            Control::Table(t) => {
                                println!(
                                    "    TABLE {}×{} ({} cells)",
                                    t.rows,
                                    t.cols,
                                    t.cells.len()
                                );
                            }
                            Control::Unknown { ctrl_id, data } => {
                                let id_bytes = ctrl_id.to_le_bytes();
                                let id_str = String::from_utf8_lossy(&id_bytes);
                                println!(
                                    "    CTRL {:?} ({:#010x}, {} bytes)",
                                    id_str,
                                    ctrl_id,
                                    data.len()
                                );
                            }
                            _ => {
                                println!("    CTRL {:?}", std::mem::discriminant(ctrl));
                            }
                        }
                    }
                }
            }

            if !doc.bin_data.is_empty() {
                println!("\n--- BinData ---");
                for entry in &doc.bin_data {
                    println!("  {}: {} bytes", entry.name, entry.data.len());
                }
            }

            println!("\nDone.");
        }
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}
