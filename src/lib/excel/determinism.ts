import JSZip from 'jszip';

/**
 * Rewrites an xlsx buffer so every ZIP entry carries a fixed epoch timestamp
 * AND so docProps/core.xml's creator/lastModifiedBy are zeroed out.
 *
 * Two determinism/PII bugs sit at the ZIP layer that no amount of
 * `workbook.creator = ''` fixes:
 *
 * 1. JSZip stamps each entry with `new Date()` at write time. Those DOS
 *    timestamps land in the central directory and local file headers, so
 *    two runs of the same workbook produce different bytes even when the
 *    pinned workbook metadata is identical.
 *
 * 2. ExcelJS replaces an empty-string `creator`/`lastModifiedBy` with the
 *    literal "Unknown" at serialization time (see
 *    exceljs/lib/xlsx/xform/core/core-xform.js). The only way to actually
 *    emit an empty string is to patch `docProps/core.xml` after ExcelJS
 *    has written it.
 *
 * Unzip, zero every entry's `date`, rewrite `docProps/core.xml` with
 * empty creator/lastModifiedBy, re-zip. Must still be paired with
 * `workbook.created = new Date(0)` + zeroed `creator`/`company`/… on
 * the ExcelJS Workbook so the OTHER fields in core.xml are also empty.
 */
export async function pinXlsxDates(buf: Buffer): Promise<Buffer> {
  const EPOCH = new Date(0);
  const zip = await JSZip.loadAsync(buf);

  // Scrub "Unknown" from docProps/core.xml. Match on the exact XML
  // ExcelJS emits — a surgical replace, not a full XML rewrite, so we
  // don't accidentally reorder nodes and break downstream byte equality.
  const coreFile = zip.file('docProps/core.xml');
  if (coreFile) {
    const xml = await coreFile.async('string');
    const scrubbed = xml
      .replace(/<dc:creator>Unknown<\/dc:creator>/g,         '<dc:creator></dc:creator>')
      .replace(/<cp:lastModifiedBy>Unknown<\/cp:lastModifiedBy>/g, '<cp:lastModifiedBy></cp:lastModifiedBy>');
    zip.file('docProps/core.xml', scrubbed);
  }

  zip.forEach((_path, file) => {
    file.date = EPOCH;
  });

  const out = await zip.generateAsync({
    type: 'nodebuffer',
    compression: 'DEFLATE',
    // Fixed-level compression gives stable output across zlib builds that
    // might otherwise choose different defaults.
    compressionOptions: { level: 6 },
  });

  return out;
}
