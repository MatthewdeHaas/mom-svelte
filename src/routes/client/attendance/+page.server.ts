import {
  getAttendanceByDate,
  getOrders,
  getPersonsByCompany,
  upsertAttendanceBatch,
} from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';
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
  const caller = callerFromLocals(locals);

  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const horizon = new Date(today.getTime() + 21 * 86400000); // 3 weeks

  const [persons, upcomingOrders] = await Promise.all([
    getPersonsByCompany(companyId, {}, caller),
    getOrders(
      {
        company_id: companyId,
        serving_date_from: today,
        serving_date_to: horizon,
      },
      caller,
    ),
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

  // Fan out across the (small, ≤21) date set — the dispatcher only
  // exposes single-date reads, and `AttendanceWithPerson` includes
  // the person FK so we can key off it directly. Company scoping is
  // enforced server-side; filter here to the persons we rendered.
  const personIdSet = new Set(persons.map((p) => p.id));
  const perDay = await Promise.all(
    dates.map((d) => getAttendanceByDate(companyId, new Date(d), caller)),
  );

  // key: "personId|YYYY-MM-DD" → attending boolean
  const attendance: Record<string, boolean> = {};
  for (let i = 0; i < dates.length; i++) {
    const date = dates[i];
    for (const row of perDay[i]) {
      if (!personIdSet.has(row.person_id)) continue;
      attendance[`${row.person_id}|${date}`] = row.attending;
    }
  }

  return { persons, dates, attendance };
};

export const actions: Actions = {
  save: async ({ request, locals }) => {
    const companyId = locals.user!.companyId!;
    const caller = callerFromLocals(locals);
    const form = await request.formData();

    // Reconstruct the full persons × dates matrix that was submitted.
    // Hidden inputs encode every cell as "personId|date" → '1'/'0'.
    // We only trust person IDs that actually belong to this company.
    const persons = await getPersonsByCompany(companyId, {}, caller);
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

    await upsertAttendanceBatch(records, caller);
    return { action: 'save', success: true };
  },
};
