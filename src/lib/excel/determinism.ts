import JSZip from 'jszip';

/**
 * Rewrites an xlsx buffer so every ZIP entry carries a fixed epoch timestamp.
 *
 * ExcelJS defers to JSZip for the actual zip, and JSZip stamps each entry
 * with `new Date()` at write time. Those DOS timestamps land in the central
 * directory and local file headers, so two runs of the same workbook produce
 * different bytes even when `workbook.created`/`workbook.modified` are pinned.
 *
 * Unzip, pin every entry's `date` to the Unix epoch, re-zip. The content
 * payloads are untouched — only entry metadata changes. Must be paired with
 * `workbook.created = new Date(0)` + zeroed `creator`/`company`/… so the
 * payload itself is also deterministic.
 */
export async function pinXlsxDates(buf: Buffer): Promise<Buffer> {
  const EPOCH = new Date(0);
  const zip = await JSZip.loadAsync(buf);

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
