import type { ContributionDay } from "../dashboard";

const DAY_MS = 86_400_000;

function parseDateOnly(value: string): Date | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) return null;
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const date = new Date(Date.UTC(year, month - 1, day));
  return date.getUTCFullYear() === year && date.getUTCMonth() === month - 1 && date.getUTCDate() === day
    ? date
    : null;
}

export function localIsoDate(date = new Date()): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

export function formatContributionDate(value: string, locale: string): string {
  const date = parseDateOnly(value);
  return date
    ? new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeZone: "UTC" }).format(date)
    : value;
}

export function positionContributionDays(days: ContributionDay[]) {
  const parsed = days.map((day) => ({ day, date: parseDateOnly(day.date) }));
  const weekStarts = parsed.flatMap(({ date }) => date
    ? [date.valueOf() - date.getUTCDay() * DAY_MS]
    : []);
  const firstWeekStart = weekStarts.length > 0 ? Math.min(...weekStarts) : null;

  return parsed.map(({ day, date }) => {
    if (!date || firstWeekStart === null) return { day, date, style: undefined };
    const weekStart = date.valueOf() - date.getUTCDay() * DAY_MS;
    return {
      day,
      date,
      style: {
        gridRow: date.getUTCDay() + 1,
        gridColumn: Math.floor((weekStart - firstWeekStart) / (7 * DAY_MS)) + 1,
      },
    };
  });
}

export function heatmapMonthLabels(days: ContributionDay[], locale: string) {
  const labels = new Map<string, { label: string; column: number }>();
  for (const entry of positionContributionDays(days)) {
    if (!entry.date || !entry.style) continue;
    const key = `${entry.date.getUTCFullYear()}-${entry.date.getUTCMonth()}`;
    if (!labels.has(key)) {
      labels.set(key, {
        label: new Intl.DateTimeFormat(locale, { month: "short", timeZone: "UTC" }).format(entry.date),
        column: entry.style.gridColumn,
      });
    }
  }
  return [...labels.values()];
}
