import { pool } from '$lib/server/db';
import { getPersonsByCompany } from '$lib/server/queries/persons';
import { getOrders } from '$lib/server/queries/orders';
import { upsertAttendanceBatch } from '$lib/server/queries/persons';
import type { UpsertAttendance } from '$lib/types';
import type { Actions, PageServerLoad } from './$types';

function toDateStr(d: Date | string): string {
  // Always format in UTC to avoid timezone shifts when pg returns dates as
  // midnight UTC and we extract year/month/day in a negative-offset timezone.
  const dt = new Date(d);
  return dt.toISOString().slice(0, 10);
}

export interface AttendanceCell {
  personId: string;
  date: string;       // YYYY-MM-DD
  attending: boolean;
}

export const load: PageServerLoad = async ({ locals }) => {
  const companyId = locals.user!.companyId!;

  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const horizon = new Date(today.getTime() + 21 * 86400000); // 3 weeks

  const [persons, upcomingOrders] = await Promise.all([
    getPersonsByCompany(pool, companyId),
    getOrders(pool, {
      company_id: companyId,
      serving_date_from: today,
      serving_date_to: horizon,
    }),
  ]);

  // Distinct serving dates from non-cancelled upcoming orders, sorted ascending
  const dateSet = new Set<string>();
  for (const o of upcomingOrders) {
    if (o.status !== 'cancelled') dateSet.add(toDateStr(o.serving_date));
  }
  const dates = [...dateSet].sort();

  if (persons.length === 0 || dates.length === 0) {
    return { persons, dates, attendance: {} as Record<string, boolean> };
  }

  // Fetch existing attendance for all person × date combos in one query
  const personIds = persons.map((p) => p.id);
  const rows = await pool.query<{ person_id: string; serving_date: Date; attending: boolean }>(
    `SELECT person_id, serving_date, attending
     FROM attendance
     WHERE person_id = ANY($1)
       AND serving_date = ANY($2::date[])`,
    [personIds, dates]
  );

  // key: "personId|YYYY-MM-DD" → attending boolean
  const attendance: Record<string, boolean> = {};
  for (const row of rows.rows) {
    attendance[`${row.person_id}|${toDateStr(row.serving_date)}`] = row.attending;
  }

  return { persons, dates, attendance };
};

export const actions: Actions = {
  save: async ({ request, locals }) => {
    const companyId = locals.user!.companyId!;
    const form = await request.formData();

    // Reconstruct the full persons × dates matrix that was submitted.
    // Hidden inputs encode every cell as "personId|date" → '1'/'0'.
    // We only trust person IDs that actually belong to this company.
    const persons = await getPersonsByCompany(pool, companyId);
    const personIdSet = new Set(persons.map((p) => p.id));

    const records: UpsertAttendance[] = [];

    for (const [key, val] of form.entries()) {
      // key format: "att|<personId>|<YYYY-MM-DD>"
      if (!key.startsWith('att|')) continue;
      const [, personId, date] = key.split('|');
      if (!personId || !date || !personIdSet.has(personId)) continue;
      records.push({
        person_id: personId,
        serving_date: date,
        attending: val === '1',
      });
    }

    if (records.length === 0) {
      return { action: 'save', success: true };
    }

    await upsertAttendanceBatch(pool, records);
    return { action: 'save', success: true };
  },
};
