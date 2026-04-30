// Timezone used for all export scoping
const TZ = 'America/New_York';

/**
 * Returns the current date in EST as a plain YYYY-MM-DD string.
 */
export function todayEST(): string {
  return new Intl.DateTimeFormat('en-CA', { timeZone: TZ }).format(new Date());
}

/**
 * Given any date string (YYYY-MM-DD), returns the Monday and Sunday
 * of the ISO week containing that date, in EST.
 */
export function weekBoundsEST(dateStr: string): { from: string; to: string } {
  // Parse as EST midnight
  const d = new Date(
    new Intl.DateTimeFormat('en-CA', { timeZone: TZ }).format(new Date(dateStr))
  );

  // Re-parse so we can do arithmetic
  const local = new Date(`${dateStr}T00:00:00`);
  const day = local.getDay(); // 0 = Sun
  const diffToMon = day === 0 ? -6 : 1 - day;

  const mon = new Date(local);
  mon.setDate(local.getDate() + diffToMon);

  const sun = new Date(mon);
  sun.setDate(mon.getDate() + 6);

  const fmt = (dt: Date) => dt.toISOString().slice(0, 10);
  return { from: fmt(mon), to: fmt(sun) };
}

/**
 * Formats a YYYY-MM-DD string for display, e.g. "28 Apr 2025"
 */
export function formatDateDisplay(dateStr: string): string {
  return new Date(`${dateStr}T00:00:00`).toLocaleDateString('en-GB', {
    day: 'numeric', month: 'short', year: 'numeric',
  });
}

/**
 * Formats a week range for display, e.g. "28 Apr – 4 May 2025"
 */
export function formatWeekDisplay(from: string, to: string): string {
  const f = new Date(`${from}T00:00:00`);
  const t = new Date(`${to}T00:00:00`);

  const sameYear = f.getFullYear() === t.getFullYear();
  const sameMonth = f.getMonth() === t.getMonth();

  const fromStr = f.toLocaleDateString('en-GB', {
    day: 'numeric',
    month: sameMonth ? undefined : 'short',
    year: sameYear ? undefined : 'numeric',
  });
  const toStr = t.toLocaleDateString('en-GB', {
    day: 'numeric', month: 'short', year: 'numeric',
  });

  return `${fromStr} – ${toStr}`;
}

/**
 * Advances a YYYY-MM-DD string by `days` days.
 */
export function shiftDate(dateStr: string, days: number): string {
  const d = new Date(`${dateStr}T00:00:00`);
  d.setDate(d.getDate() + days);
  return d.toISOString().slice(0, 10);
}
