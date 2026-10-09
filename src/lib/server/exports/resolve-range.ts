import { error } from '@sveltejs/kit';
import { weekBoundsEST } from '$lib/utils/dates';

/**
 * Query-shape aliases for the admin exports routes.
 *
 * The Rust exports API takes from and to on xlsx endpoints and date on
 * packing-slips. The pre-port SvelteKit routes took a single shipment_date
 * with week-of semantics, and the admin UI plus the e2e specs still emit
 * that shape. These helpers accept all three forms and normalise to what
 * Rust expects, so the HTTP wrappers in the three export routes stay dumb.
 */

const DATE_RE = /^\d{4}-\d{2}-\d{2}$/;

export interface ExportRange {
  from: string;
  to: string;
}

/** For production-order and shopping-list (xlsx). Rust takes from and to. */
export function resolveExportRange(url: URL): ExportRange {
  const from = url.searchParams.get('from');
  const to = url.searchParams.get('to');
  if (from && to) {
    if (!DATE_RE.test(from) || !DATE_RE.test(to))
      throw error(400, 'from and to must be YYYY-MM-DD');
    return { from, to };
  }
  const weekOf = url.searchParams.get('week_of');
  if (weekOf) {
    if (!DATE_RE.test(weekOf)) throw error(400, 'week_of must be YYYY-MM-DD');
    return weekBoundsEST(weekOf);
  }
  const shipmentDate = url.searchParams.get('shipment_date');
  if (shipmentDate) {
    if (!DATE_RE.test(shipmentDate))
      throw error(400, 'shipment_date must be YYYY-MM-DD');
    return weekBoundsEST(shipmentDate);
  }
  throw error(400, 'provide from+to, week_of, or shipment_date (YYYY-MM-DD)');
}

/** For packing-slips. Rust takes a single date. */
export function resolvePackingSlipsDate(url: URL): string {
  const date =
    url.searchParams.get('date') ??
    url.searchParams.get('week_of') ??
    url.searchParams.get('shipment_date');
  if (!date)
    throw error(400, 'provide date, week_of, or shipment_date (YYYY-MM-DD)');
  if (!DATE_RE.test(date)) throw error(400, 'date must be YYYY-MM-DD');
  return date;
}
