//! Byte-identical xlsx post-processor.
//!
//! Port of `src/lib/excel/determinism.ts::pinXlsxDates`. `rust_xlsxwriter`
//! (like ExcelJS) stamps every ZIP entry with the current time at write
//! time, so two generations of the same semantic data produce different
//! bytes. On top of that, `rust_xlsxwriter` emits `<dc:creator>Unknown`
//! and `<cp:lastModifiedBy>Unknown` into `docProps/core.xml` whenever
//! those metadata fields are left blank — the TS pipeline hits the same
//! wall with ExcelJS's "Unknown" fallback.
//!
//! This module runs as the final step inside both xlsx generators:
//!
//! 1. Load the xlsx bytes as a ZIP archive from an in-memory cursor.
//! 2. For `docProps/core.xml`, surgically string-replace the two
//!    "Unknown" strings with empty content — a textual replace rather
//!    than an XML re-emission so the node order (and therefore the
//!    byte layout) stays stable regardless of upstream XML library
//!    quirks.
//! 3. Walk every entry, re-write to a new in-memory ZIP with
//!    `last_modified_time` pinned to the DOS epoch (1980-01-01 00:00:00,
//!    which is `zip::DateTime::default()`), compression `Deflated`,
//!    `compression_level = 6`.
//!
//! The compression level is pinned because different zlib builds pick
//! different defaults; without pinning, two machines could produce
//! byte-divergent output from identical semantic input and the Rust
//! determinism tests would flap on CI vs local. Level 6 matches the TS
//! pipeline's `compressionOptions: { level: 6 }`.

use std::io::{Cursor, Read, Write};

use zip::{
    write::{FileOptions, ZipWriter},
    CompressionMethod, DateTime, ZipArchive,
};

use crate::export::ExportError;

/// Rewrites xlsx bytes so two runs of the same semantic data produce
/// byte-identical output. See module docs for the algorithm.
pub fn pin_xlsx_bytes(bytes: Vec<u8>) -> Result<Vec<u8>, ExportError> {
    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).map_err(ExportError::Zip)?;

    // Pinned entry mtime — DOS epoch. `DateTime::default()` is
    // 1980-01-01 00:00:00, which is the minimum DOS timestamp (the ZIP
    // format can't go below that), matching the semantics of
    // `new Date(0)` → clamped-to-DOS-epoch on the TS side via JSZip.
    let pinned_mtime = DateTime::default();
    let options: FileOptions = FileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6))
        .last_modified_time(pinned_mtime);

    let mut out: Vec<u8> = Vec::with_capacity(archive.len() * 2048);
    {
        let out_cursor = Cursor::new(&mut out);
        let mut writer = ZipWriter::new(out_cursor);

        // ZIP preserves insertion order; iterate by index so the output
        // entry sequence matches the input sequence.
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(ExportError::Zip)?;
            let name = entry.name().to_owned();
            let is_dir = entry.is_dir();

            let mut buf: Vec<u8> = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut buf).map_err(ExportError::Io)?;
            drop(entry);

            if is_dir {
                writer
                    .add_directory(&name, options)
                    .map_err(ExportError::Zip)?;
                continue;
            }

            // Scrub "Unknown" from docProps/core.xml. Textual replace
            // on the raw UTF-8 bytes — the strings are ASCII, so no
            // encoding surprise. Matches TS line-for-line.
            if name == "docProps/core.xml" {
                let xml = String::from_utf8(buf).map_err(ExportError::Utf8)?;
                let scrubbed = xml
                    .replace(
                        "<dc:creator>Unknown</dc:creator>",
                        "<dc:creator></dc:creator>",
                    )
                    .replace(
                        "<cp:lastModifiedBy>Unknown</cp:lastModifiedBy>",
                        "<cp:lastModifiedBy></cp:lastModifiedBy>",
                    );
                buf = scrubbed.into_bytes();
            }

            writer
                .start_file(&name, options)
                .map_err(ExportError::Zip)?;
            writer.write_all(&buf).map_err(ExportError::Io)?;
        }

        writer.finish().map_err(ExportError::Zip)?;
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smallest viable test — build a 1-entry ZIP by hand, pin it
    /// twice, assert the bytes are identical. Guards against a `zip`
    /// crate upgrade that silently starts stamping entries with the
    /// current time regardless of `last_modified_time`.
    #[test]
    fn pins_bytes_across_runs() {
        let mut seed: Vec<u8> = Vec::new();
        {
            let cursor = Cursor::new(&mut seed);
            let mut w = ZipWriter::new(cursor);
            // Deliberately write with a non-epoch mtime so the pin pass
            // has something to normalise.
            let opts: FileOptions = FileOptions::default()
                .compression_method(CompressionMethod::Deflated)
                .compression_level(Some(6))
                .last_modified_time(DateTime::from_date_and_time(2024, 6, 1, 12, 0, 0).unwrap());
            w.start_file("hello.txt", opts).unwrap();
            w.write_all(b"world").unwrap();
            w.finish().unwrap();
        }

        let a = pin_xlsx_bytes(seed.clone()).unwrap();
        let b = pin_xlsx_bytes(seed).unwrap();
        assert_eq!(a, b, "pinned xlsx bytes must be byte-identical");
    }

    /// Structural snapshot of the pinned ZIP — sorted
    /// `(name, size, crc32)` tuples. The hard byte-equality assert in
    /// `pins_bytes_across_runs` fails first on any drift; this
    /// snapshot runs after it and gives a reviewable diff of what
    /// structurally changed (new entry? different compressed size?
    /// crc flipped?) instead of an unreadable byte-array dump. Run
    /// `cargo insta review` to accept intentional changes.
    #[test]
    fn snapshot_pinned_zip_structure() {
        let mut seed: Vec<u8> = Vec::new();
        {
            let cursor = Cursor::new(&mut seed);
            let mut w = ZipWriter::new(cursor);
            let opts: FileOptions = FileOptions::default()
                .compression_method(CompressionMethod::Deflated)
                .compression_level(Some(6))
                .last_modified_time(DateTime::from_date_and_time(2024, 6, 1, 12, 0, 0).unwrap());
            w.start_file("hello.txt", opts).unwrap();
            w.write_all(b"world").unwrap();
            w.finish().unwrap();
        }

        let pinned = pin_xlsx_bytes(seed).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(pinned)).unwrap();
        let mut entries: Vec<(String, u64, u32)> = (0..archive.len())
            .map(|i| {
                let e = archive.by_index(i).unwrap();
                (e.name().to_string(), e.size(), e.crc32())
            })
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        insta::assert_yaml_snapshot!(entries);
    }

    /// Scrubs the "Unknown" strings when they land in `docProps/core.xml`.
    #[test]
    fn scrubs_unknown_in_core_xml() {
        let mut raw: Vec<u8> = Vec::new();
        {
            let cursor = Cursor::new(&mut raw);
            let mut w = ZipWriter::new(cursor);
            let opts: FileOptions = FileOptions::default()
                .compression_method(CompressionMethod::Deflated)
                .compression_level(Some(6));
            w.start_file("docProps/core.xml", opts).unwrap();
            w.write_all(
                b"<cp:coreProperties><dc:creator>Unknown</dc:creator>\
                  <cp:lastModifiedBy>Unknown</cp:lastModifiedBy></cp:coreProperties>",
            )
            .unwrap();
            w.finish().unwrap();
        }

        let pinned = pin_xlsx_bytes(raw).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(pinned)).unwrap();
        let mut entry = archive.by_name("docProps/core.xml").unwrap();
        let mut xml = String::new();
        entry.read_to_string(&mut xml).unwrap();
        assert!(!xml.contains(">Unknown<"), "'Unknown' should be scrubbed");
        assert!(xml.contains("<dc:creator></dc:creator>"));
        assert!(xml.contains("<cp:lastModifiedBy></cp:lastModifiedBy>"));
    }
}
