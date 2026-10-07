// _semantic.ts — semantic determinism oracle for xlsx and pdf exports.
//
// GOAL: produce a stable, implementation-insensitive hash of the LOGICAL
// content of an export, so a TS writer and a (future) Rust writer that both
// claim to emit the same workbook/document hash the same.
//
// EXPLICIT NON-GOALS:
//   - This is NOT a visual-regression oracle. We do NOT capture fills, fonts,
//     alignment, borders, colors, or font sizes. Any such regression requires
//     a separate pixel-diff test.
//   - We do NOT capture PDF x/y coordinates — pdfkit uses exact glyph widths,
//     lopdf (Rust) will use ~0.5em-per-char approximations, and the layout
//     diverges while the logical text remains identical. We use the PDF text
//     positioning operators (Td/TD/Tm/T*) ONLY as logical-run separators and
//     DROP their numeric operands entirely.
//
// INVARIANTS the oracle intentionally LOCKS DOWN:
//   - Column index + header pairing per cell: a column-swap-with-same-values
//     bug flips the hash (header travels with the value).
//   - Row order within a sheet (row order IS the artifact under test).
//   - Sheet enumeration via localeCompare on sheet name (not insertion order).
//   - PDF per-page run order; adjacent text ops with no intervening Td/TD/Tm/
//     T* are concatenated into one run (defends against pdfkit batching glyphs
//     into separate Tj calls).
//
// NEGATIVE CONTROL: tests/unit/semantic-oracle.test.ts asserts a single-cell
// / single-run text mutation flips the hash, and that dropped-metadata
// mutations do NOT flip the hash. Any PR that over-normalizes will fail the
// cell-mutation case.
//
// WHEN YOU UPGRADE pdfkit OR exceljs: re-run the negative-control suite. If
// hashes drift without a logical content change, the writer's operator
// stream shape changed and you need to re-bless or extend the walker.
//
// DEBUG: set SEMANTIC_DEBUG=1 to dump the NDJSON to
// /tmp/semantic-{kind}-{sha256-prefix}.ndjson so you can diff two inputs.

import crypto from 'crypto';
import fs from 'fs';
import path from 'path';
import ExcelJS from 'exceljs';

// ---------------------------------------------------------------------------
// Shared NDJSON canonicalizer + hasher
// ---------------------------------------------------------------------------

/** JSON.stringify with recursively key-sorted object serialization. */
function stableStringify(value: unknown): string {
  return JSON.stringify(value, (_key, val) => {
    if (val && typeof val === 'object' && !Array.isArray(val)) {
      const sorted: Record<string, unknown> = {};
      for (const k of Object.keys(val as Record<string, unknown>).sort()) {
        sorted[k] = (val as Record<string, unknown>)[k];
      }
      return sorted;
    }
    return val;
  });
}

function hashNdjson(records: unknown[], kind: 'xlsx' | 'pdf'): string {
  const ndjson = records.map(stableStringify).join('\n');
  const digest = crypto.createHash('sha256').update(ndjson, 'utf8').digest('hex');
  if (process.env.SEMANTIC_DEBUG === '1') {
    try {
      const out = path.join('/tmp', `semantic-${kind}-${digest.slice(0, 12)}.ndjson`);
      fs.writeFileSync(out, ndjson, 'utf8');
    } catch {
      // best-effort debug dump; never let it break the oracle
    }
  }
  return digest;
}

// ---------------------------------------------------------------------------
// XLSX extractor (ExcelJS read-back)
// ---------------------------------------------------------------------------

type CellType = 'n' | 's' | 'd' | 'b' | 'null' | 'f' | 'err';

interface XlsxCellRecord {
  r: number;
  c: number;
  addr: string;
  header: string;
  t: CellType;
  v: string | number | null;
}

interface XlsxColumnRecord {
  index: number;
  header: string;
  width: number | null;
  numFmt: string | null;
}

interface XlsxSheetRecord {
  kind: 'xlsx-sheet';
  name: string;
  dims: { rowCount: number; colCount: number };
  frozen: { xSplit: number; ySplit: number };
  columns: XlsxColumnRecord[];
  cells: XlsxCellRecord[];
}

function normalizeNumber(n: number): number | string {
  if (!Number.isFinite(n)) {
    throw new Error(`semantic oracle: refuse to hash non-finite number: ${n}`);
  }
  if (Number.isInteger(n)) return n;
  const s = n.toFixed(6).replace(/\.?0+$/, '');
  return s === '' || s === '-' ? '0' : s;
}

function toYmdFromDate(d: Date): string {
  // Zero out any stray time-of-day: reconstruct via UTC year/month/day.
  const y = d.getUTCFullYear();
  const m = String(d.getUTCMonth() + 1).padStart(2, '0');
  const day = String(d.getUTCDate()).padStart(2, '0');
  return `${y}-${m}-${day}`;
}

function normalizeCell(
  cell: ExcelJS.Cell,
): { t: CellType; v: string | number | null } {
  const raw = cell.value;

  if (raw === null || raw === undefined) {
    return { t: 'null', v: null };
  }

  // Formulas: emit the formula text, not the result. Current writers emit
  // no formulas; this is forward-defensive.
  if (typeof raw === 'object' && raw !== null && 'formula' in raw) {
    const f = (raw as ExcelJS.CellFormulaValue).formula;
    return { t: 'f', v: typeof f === 'string' ? f.normalize('NFC') : String(f) };
  }

  // Error cells
  if (typeof raw === 'object' && raw !== null && 'error' in raw) {
    return { t: 'err', v: String((raw as ExcelJS.CellErrorValue).error) };
  }

  // Rich text — flatten to NFC string
  if (typeof raw === 'object' && raw !== null && 'richText' in raw) {
    const parts = (raw as ExcelJS.CellRichTextValue).richText.map((r) => r.text);
    return { t: 's', v: parts.join('').normalize('NFC') };
  }

  // Hyperlink — fall through on .text
  if (typeof raw === 'object' && raw !== null && 'text' in raw && 'hyperlink' in raw) {
    const t = (raw as ExcelJS.CellHyperlinkValue).text;
    return { t: 's', v: (typeof t === 'string' ? t : String(t)).normalize('NFC') };
  }

  if (raw instanceof Date) {
    return { t: 'd', v: toYmdFromDate(raw) };
  }

  if (typeof raw === 'number') {
    const n = normalizeNumber(raw);
    // Keep the type tag 'n' and emit either JSON number or string.
    return { t: 'n', v: n };
  }

  if (typeof raw === 'boolean') {
    return { t: 'b', v: raw ? 'true' : 'false' };
  }

  if (typeof raw === 'string') {
    // Future Rust ISO-date path: cell may arrive as a string.
    if (/^\d{4}-\d{2}-\d{2}/.test(raw)) {
      const d = new Date(raw.slice(0, 10) + 'T00:00:00Z');
      if (!Number.isNaN(d.getTime())) {
        return { t: 'd', v: toYmdFromDate(d) };
      }
    }
    return { t: 's', v: raw.normalize('NFC') };
  }

  // Fallback: coerce to string
  return { t: 's', v: String(raw).normalize('NFC') };
}

export async function semanticHashXlsx(buf: Buffer): Promise<string> {
  const wb = new ExcelJS.Workbook();
  // ExcelJS accepts Buffer-like; cast to any to satisfy the typing (its
  // declared Buffer type confuses TS when passed a Node Buffer).
  await wb.xlsx.load(buf as unknown as ExcelJS.Buffer);

  const sheets = [...wb.worksheets];
  const collator = new Intl.Collator('en');
  sheets.sort((a, b) => collator.compare(a.name, b.name));

  const records: XlsxSheetRecord[] = [];

  for (const ws of sheets) {
    // Build header lookup from row 1
    const headerMap = new Map<number, string>();
    const row1 = ws.getRow(1);
    row1.eachCell({ includeEmpty: false }, (cell, colNum) => {
      const v = cell.value;
      if (v === null || v === undefined) {
        headerMap.set(colNum, '');
      } else if (typeof v === 'string') {
        headerMap.set(colNum, v.normalize('NFC'));
      } else if (typeof v === 'object' && v !== null && 'richText' in v) {
        headerMap.set(
          colNum,
          (v as ExcelJS.CellRichTextValue).richText
            .map((r) => r.text)
            .join('')
            .normalize('NFC'),
        );
      } else {
        headerMap.set(colNum, String(v).normalize('NFC'));
      }
    });

    // Columns: index, header, width, numFmt (date detection only)
    const columns: XlsxColumnRecord[] = [];
    const colCount = ws.columnCount;
    for (let c = 1; c <= colCount; c++) {
      const col = ws.getColumn(c);
      const width = typeof col.width === 'number' ? col.width : null;
      // Only keep numFmt if it looks like a date format — otherwise drop.
      const rawFmt =
        typeof col.numFmt === 'string' ? col.numFmt : null;
      const numFmt =
        rawFmt && /[dmyh]/i.test(rawFmt) ? rawFmt : null;
      columns.push({
        index: c,
        header: headerMap.get(c) ?? '',
        width,
        numFmt,
      });
    }

    const cells: XlsxCellRecord[] = [];
    ws.eachRow({ includeEmpty: false }, (row, rowNum) => {
      row.eachCell({ includeEmpty: false }, (cell, colNum) => {
        const { t, v } = normalizeCell(cell);
        cells.push({
          r: rowNum,
          c: colNum,
          addr: cell.address,
          header: headerMap.get(colNum) ?? '',
          t,
          v,
        });
      });
    });

    // Cells arrive in (row asc, col asc) order from ExcelJS iteration.
    const view = ws.views?.[0];
    const xSplit =
      view && 'xSplit' in view && typeof view.xSplit === 'number'
        ? view.xSplit
        : 0;
    const ySplit =
      view && 'ySplit' in view && typeof view.ySplit === 'number'
        ? view.ySplit
        : 0;

    records.push({
      kind: 'xlsx-sheet',
      name: ws.name,
      dims: { rowCount: ws.rowCount, colCount: ws.columnCount },
      frozen: { xSplit, ySplit },
      columns,
      cells,
    });
  }

  return hashNdjson(records, 'xlsx');
}

// ---------------------------------------------------------------------------
// PDF extractor (hand-rolled walker over uncompressed content streams)
// ---------------------------------------------------------------------------
//
// Precondition: packing-slip.ts emits with compress:false (see
// src/lib/pdf/packing-slip.ts:478). The buffer is after patchPageTotal,
// so the real page total has already replaced ##TOTAL##.
//
// FALLBACK PLAN (not implemented up front): if a future pdfkit upgrade makes
// this walker brittle, swap to the `pdf-parse` package (pure-JS, no worker
// setup). The oracle's output shape would stay the same; only the extraction
// pipeline changes.

interface PdfPageRecord {
  kind: 'pdf-page';
  page: number;
  runs: string[];
}
interface PdfInfoRecord {
  kind: 'pdf-info';
  title: string;
  subject: string;
  producer: string;
  creator: string;
}
type PdfRecord = PdfPageRecord | PdfInfoRecord;

/** Decode a PDF literal string body (contents between parens). */
function decodePdfString(body: string): string {
  let out = '';
  for (let i = 0; i < body.length; i++) {
    const ch = body[i];
    if (ch !== '\\') {
      out += ch;
      continue;
    }
    const next = body[i + 1];
    if (next === undefined) break;
    // Octal escape: up to 3 digits
    if (/[0-7]/.test(next)) {
      let oct = next;
      let j = i + 2;
      while (oct.length < 3 && j < body.length && /[0-7]/.test(body[j])) {
        oct += body[j];
        j++;
      }
      out += String.fromCharCode(parseInt(oct, 8));
      i = j - 1;
      continue;
    }
    switch (next) {
      case 'n': out += '\n'; break;
      case 'r': out += '\r'; break;
      case 't': out += '\t'; break;
      case 'b': out += '\b'; break;
      case 'f': out += '\f'; break;
      case '\\': out += '\\'; break;
      case '(': out += '('; break;
      case ')': out += ')'; break;
      case '\n': /* line continuation */ break;
      case '\r':
        if (body[i + 2] === '\n') i++;
        break;
      default: out += next; break;
    }
    i++;
  }
  return out;
}

/**
 * Scan from `start` and return the balanced-paren PDF literal string body
 * plus the index just past the closing ')'. Assumes buf[start] === '('.
 */
function readParenString(buf: string, start: number): { body: string; end: number } {
  let depth = 0;
  let i = start;
  while (i < buf.length) {
    const ch = buf[i];
    if (ch === '\\') {
      i += 2;
      continue;
    }
    if (ch === '(') {
      depth++;
      i++;
      continue;
    }
    if (ch === ')') {
      depth--;
      i++;
      if (depth === 0) return { body: buf.slice(start + 1, i - 1), end: i };
      continue;
    }
    i++;
  }
  return { body: buf.slice(start + 1), end: buf.length };
}

/**
 * Scan from `start` where buf[start] === '<' and return the hex-string body
 * decoded as a latin1 character sequence. pdfkit emits text as hex strings
 * like <48656c6c6f> which are the raw glyph codes under the current font.
 *
 * Hex strings skip whitespace between digits and treat a trailing odd digit
 * as if followed by '0' per the PDF spec.
 */
function readHexString(buf: string, start: number): { body: string; end: number } {
  let i = start + 1;
  let hex = '';
  while (i < buf.length && buf[i] !== '>') {
    const ch = buf[i];
    if (/[0-9A-Fa-f]/.test(ch)) hex += ch;
    i++;
  }
  if (hex.length % 2 === 1) hex += '0';
  let out = '';
  for (let j = 0; j < hex.length; j += 2) {
    out += String.fromCharCode(parseInt(hex.substr(j, 2), 16));
  }
  return { body: out, end: i + 1 };
}

const MONTHS: Record<string, number> = {
  january: 1, february: 2, march: 3, april: 4, may: 5, june: 6,
  july: 7, august: 8, september: 9, october: 10, november: 11, december: 12,
};

function normalizePdfRun(run: string): string | null {
  // NFC + collapse whitespace (incl. nbsp) + trim
  let s = run.normalize('NFC').replace(/[\s ]+/g, ' ').trim();
  if (!s) return null;

  // Normalize "Page N of         M" (patchPageTotal uses padStart(9,' '))
  s = s.replace(/\bPage\s+(\d+)\s+of\s+(\d+)\b/g, 'Page $1 of $2');

  // Locale date rewrite: "Monday, March 3, 2026" or "March 3, 2026"
  const localeMatch = s.match(
    /^(?:(?:Monday|Tuesday|Wednesday|Thursday|Friday|Saturday|Sunday),\s*)?(January|February|March|April|May|June|July|August|September|October|November|December)\s+(\d{1,2}),\s*(\d{4})$/i,
  );
  if (localeMatch) {
    const mon = MONTHS[localeMatch[1].toLowerCase()];
    const day = parseInt(localeMatch[2], 10);
    const year = parseInt(localeMatch[3], 10);
    const d = new Date(Date.UTC(year, mon - 1, day));
    if (Number.isNaN(d.getTime())) {
      throw new Error(`semantic oracle: failed to parse locale date "${s}"`);
    }
    s = toYmdFromDate(d);
  }

  return s;
}

/** Walk a single content stream, returning an ordered list of text runs. */
function walkContentStream(stream: string): string[] {
  const runs: string[] = [];
  // Find each BT ... ET block
  const btRe = /BT\b([\s\S]*?)\bET\b/g;
  let btMatch: RegExpExecArray | null;
  while ((btMatch = btRe.exec(stream)) !== null) {
    const body = btMatch[1];
    let i = 0;
    let currentRun = '';
    const flush = () => {
      const normalized = normalizePdfRun(currentRun);
      if (normalized !== null) runs.push(normalized);
      currentRun = '';
    };

    while (i < body.length) {
      const ch = body[i];

      // String literal (…) followed later by Tj / ' / "
      if (ch === '(') {
        const { body: strBody, end } = readParenString(body, i);
        currentRun += decodePdfString(strBody);
        i = end;
        continue;
      }

      // Hex string <…> — pdfkit emits text this way under embedded fonts.
      // The content between < > is a sequence of 2-digit hex codes mapping
      // to glyph indices under the active font. Since we don't resolve the
      // font's CMap, we fall back to the raw character bytes — this works
      // for ASCII text because pdfkit's embedded subsets preserve ASCII
      // code points. The important property for the oracle is that
      // DIFFERENT TEXT YIELDS DIFFERENT HEX, which is sufficient to detect
      // content mutations.
      if (ch === '<' && body[i + 1] !== '<') {
        const { body: strBody, end } = readHexString(body, i);
        currentRun += strBody;
        i = end;
        continue;
      }

      // Array for TJ: [(...) num (...) …] TJ, or [<HEX> num <HEX> …] TJ
      if (ch === '[') {
        let j = i + 1;
        while (j < body.length && body[j] !== ']') {
          if (body[j] === '(') {
            const { body: strBody, end } = readParenString(body, j);
            currentRun += decodePdfString(strBody);
            j = end;
          } else if (body[j] === '<' && body[j + 1] !== '<') {
            const { body: strBody, end } = readHexString(body, j);
            currentRun += strBody;
            j = end;
          } else {
            j++;
          }
        }
        i = j + 1; // past ']'
        continue;
      }

      // Look for positioning operators (Td / TD / Tm / T*) — flush before.
      // These are tokens surrounded by whitespace or at line edges.
      if ((ch === 'T' || ch === 't') && i > 0) {
        const prev = body[i - 1];
        if (/\s/.test(prev) || prev === ')' || prev === ']') {
          const two = body.substr(i, 2);
          const three = body.substr(i, 3);
          const afterTwo = body[i + 2];
          const afterThree = body[i + 3];
          const isPositioning =
            ((two === 'Td' || two === 'TD' || two === 'Tm') &&
              (afterTwo === undefined || /[\s\r\n]/.test(afterTwo))) ||
            (three === 'T* ' || three === 'T*\n' || three === 'T*\r' ||
              (two === 'T*' && (afterTwo === undefined)));
          if (isPositioning) {
            flush();
            // Skip the operator token
            i += two === 'T*' ? 2 : 2;
            continue;
          }
        }
      }

      i++;
    }
    flush();
  }
  return runs;
}

function extractPdfInfo(buf: string): PdfInfoRecord {
  const info: PdfInfoRecord = {
    kind: 'pdf-info',
    title: '',
    subject: '',
    producer: '',
    creator: '',
  };

  // Find /Info N 0 R in trailer
  const trailerMatch = buf.match(/\/Info\s+(\d+)\s+0\s+R/);
  if (!trailerMatch) return info;
  const objNum = trailerMatch[1];

  // Find that indirect object
  const objRe = new RegExp(
    `\\b${objNum}\\s+0\\s+obj\\b([\\s\\S]*?)\\bendobj\\b`,
  );
  const objMatch = buf.match(objRe);
  if (!objMatch) return info;
  const objBody = objMatch[1];

  const keys = ['Title', 'Subject', 'Producer', 'Creator'] as const;
  for (const key of keys) {
    const keyRe = new RegExp(`/${key}\\s*\\(`);
    const m = objBody.match(keyRe);
    if (!m || m.index === undefined) continue;
    const parenStart = m.index + m[0].length - 1;
    const { body } = readParenString(objBody, parenStart);
    const decoded = decodePdfString(body).normalize('NFC');
    (info as unknown as Record<string, string>)[key.toLowerCase()] = decoded;
  }

  return info;
}

export function semanticHashPdf(buf: Buffer): string {
  const text = buf.toString('latin1');

  // Find every content stream object: `N 0 obj << … >> stream\n…\nendstream`
  // pdfkit emits streams with a leading LF and trailing LF before `endstream`.
  const streamRe = /\bobj\b[\s\S]*?<<[\s\S]*?>>\s*stream\r?\n([\s\S]*?)\r?\nendstream\b/g;
  const streams: string[] = [];
  let m: RegExpExecArray | null;
  while ((m = streamRe.exec(text)) !== null) {
    streams.push(m[1]);
  }

  // A single pdfkit document has one content stream per page in file order.
  // Heuristic: streams that contain at least one BT…ET are text/content
  // streams. Streams without any BT are font programs, images, etc. — skip.
  const pageRecords: PdfPageRecord[] = [];
  let pageIndex = 0;
  for (const s of streams) {
    if (!/\bBT\b/.test(s)) continue;
    pageIndex++;
    const runs = walkContentStream(s);
    pageRecords.push({ kind: 'pdf-page', page: pageIndex, runs });
  }

  const info = extractPdfInfo(text);

  const records: PdfRecord[] = [...pageRecords, info];
  return hashNdjson(records, 'pdf');
}
