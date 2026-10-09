//! Byte-identical PDF post-processor.
//!
//! `printpdf` 0.7 injects three 32-char `[A-J]{32}` random strings on
//! every save, none of which has a builder setter we're using:
//!
//! 1. **XMP `instance_id`** — written into the XMP `Metadata` stream
//!    via a `uuid:{instance}` placeholder in the template at
//!    `printpdf/assets/catalog_xmp_metadata.txt`. Lives inside a
//!    `Type=Metadata, Subtype=XML` stream object. The stream is
//!    uncompressed in debug builds and compressed (FlateDecode) in
//!    release when `optimize()` runs. `lopdf::Stream::decompressed_content`
//!    + `set_plain_content` handles both paths.
//! 2. **Trailer `/ID` first element (`document_id`)** — written in
//!    `pdf_document.rs` as `String(document_id.as_bytes(), Literal)`.
//! 3. **Trailer `/ID` second element (`instance_id`)** — same shape,
//!    minted alongside the document_id at save time.
//!
//! Both leak wall-clock randomness through to the final bytes. This
//! module reloads the saved PDF with `printpdf::lopdf`, rewrites both
//! to a pinned value, and re-serializes. No new dep — `lopdf` is
//! re-exported from `printpdf` via `pub use lopdf;`.
//!
//! The companion xlsx post-processor lives at
//! `admin-operations-rust/crates/excel/src/determinism.rs`.

use printpdf::lopdf::{self, Object, StringFormat};

use crate::PdfError;

/// Pinned replacement for the two random `[A-J]{32}` strings. The
/// specific value doesn't matter; what matters is that it's identical
/// across runs. Pure ASCII so it round-trips through PDF Literal
/// strings without escaping.
const PINNED_INSTANCE_ID: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

/// Rewrite the saved PDF so two runs with the same semantic input
/// produce byte-identical output. See module docs for the algorithm.
pub fn pin_pdf_bytes(bytes: Vec<u8>) -> Result<Vec<u8>, PdfError> {
    let mut doc = lopdf::Document::load_mem(&bytes)
        .map_err(|e| PdfError::Build(format!("lopdf load: {e}")))?;

    // ----- 1. Trailer /ID [document_id, instance_id] -------------------
    //
    // Shape produced by printpdf: `/ID [ (document_id) (instance_id) ]`
    // Both are Literal strings. `with_document_id` is a no-op path we
    // don't use, and `instance_id` is minted at save time — so BOTH
    // elements are random `[A-J]{32}` in our output. Pin both.
    //
    // If the shape doesn't match (foreign PDF, future printpdf change),
    // skip silently — the determinism tests catch the regression.
    if let Ok(id_obj) = doc.trailer.get_mut(b"ID") {
        if let Object::Array(arr) = id_obj {
            if arr.len() == 2 {
                arr[0] = Object::String(
                    PINNED_INSTANCE_ID.as_bytes().to_vec(),
                    StringFormat::Literal,
                );
                arr[1] = Object::String(
                    PINNED_INSTANCE_ID.as_bytes().to_vec(),
                    StringFormat::Literal,
                );
            }
        }
    }

    // ----- 2. XMP Metadata stream `uuid:{instance}` field --------------
    //
    // Find every stream whose dict carries `Type=Metadata`, decode its
    // content (handles the release-mode FlateDecode path), replace the
    // placeholder, and write back as plain (uncompressed) content so
    // we don't have to re-run the compressor and risk divergence from
    // the original.
    //
    // Collecting ids first avoids holding an immutable borrow across
    // the mutable `get_object_mut` call below.
    let metadata_ids: Vec<lopdf::ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| match obj {
            Object::Stream(s) => {
                let is_metadata = s
                    .dict
                    .get(b"Type")
                    .ok()
                    .and_then(|o| o.as_name_str().ok())
                    == Some("Metadata");
                if is_metadata {
                    Some(*id)
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect();

    for id in metadata_ids {
        let obj = doc
            .get_object_mut(id)
            .map_err(|e| PdfError::Build(format!("lopdf metadata lookup: {e}")))?;
        if let Object::Stream(stream) = obj {
            // `decompressed_content` handles both the uncompressed
            // (debug) and FlateDecode (release) paths.
            let raw = stream
                .decompressed_content()
                .unwrap_or_else(|_| stream.content.clone());
            let xml = match String::from_utf8(raw) {
                Ok(s) => s,
                Err(_) => continue, // non-UTF8 metadata is not ours; leave it alone
            };

            let scrubbed = scrub_xmp_instance_id(&xml);
            // `set_plain_content` strips `Filter` + `DecodeParms` and
            // writes raw bytes. Resulting PDF is slightly larger than
            // the compressed path but byte-identical across runs.
            stream.set_plain_content(scrubbed.into_bytes());
        }
    }

    let mut out: Vec<u8> = Vec::with_capacity(bytes.len() + 128);
    doc.save_to(&mut out)
        .map_err(|e| PdfError::Build(format!("lopdf save: {e}")))?;
    Ok(out)
}

/// Replace the random `uuid:XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX` inside
/// `<xmpMM:InstanceID>` with a pinned value. The template emits:
///
///   <xmpMM:InstanceID>uuid:{instance}</xmpMM:InstanceID>
///
/// where `{instance}` is 32 chars from `[A-J]`. We locate the opening
/// tag, skip past the `uuid:` prefix, and replace the next 32 bytes.
/// Textual replace rather than XML re-emission preserves node order
/// and whitespace — byte-identity holds regardless of what the parser
/// would normalize.
fn scrub_xmp_instance_id(xml: &str) -> String {
    const OPEN_TAG: &str = "<xmpMM:InstanceID>uuid:";
    let Some(start) = xml.find(OPEN_TAG) else {
        return xml.to_string();
    };
    let id_start = start + OPEN_TAG.len();
    let id_end = id_start + 32;
    if id_end > xml.len() {
        return xml.to_string();
    }
    let mut out = String::with_capacity(xml.len());
    out.push_str(&xml[..id_start]);
    out.push_str(PINNED_INSTANCE_ID);
    out.push_str(&xml[id_end..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_replaces_32_char_instance_id() {
        let xml =
            "<xmpMM:InstanceID>uuid:ABCDEFGHIJABCDEFGHIJABCDEFGHIJAB</xmpMM:InstanceID>".to_string();
        let out = scrub_xmp_instance_id(&xml);
        assert!(out.contains(&format!(
            "<xmpMM:InstanceID>uuid:{PINNED_INSTANCE_ID}</xmpMM:InstanceID>"
        )));
    }

    #[test]
    fn scrub_is_a_noop_when_tag_absent() {
        let xml = "<some><other/></some>".to_string();
        assert_eq!(scrub_xmp_instance_id(&xml), xml);
    }
}
