export function parseDateNear(
  value: string,
  referenceIso: string,
  maximumDistanceDays?: number
): string | undefined {
  const clean = value.trim();
  let year: number | undefined;
  let month: number;
  let day: number;

  let match = clean.match(/^(\d{4})[-/](\d{1,2})[-/](\d{1,2})$/);
  if (match) {
    year = Number(match[1]);
    month = Number(match[2]);
    day = Number(match[3]);
  } else {
    match = clean.match(/^(\d{1,2})[./-](\d{1,2})(?:[./-](\d{2,4}))?\.?$/);
    if (!match) return undefined;
    day = Number(match[1]);
    month = Number(match[2]);
    year = match[3] ? Number(match[3]) : undefined;
    if (year !== undefined && year < 100) year += 2000;
  }

  const reference = new Date(`${referenceIso}T12:00:00Z`);
  const years =
    year !== undefined
      ? [year]
      : [
          reference.getUTCFullYear() - 1,
          reference.getUTCFullYear(),
          reference.getUTCFullYear() + 1
        ];
  const candidates = years
    .map(candidateYear => {
      const date = new Date(Date.UTC(candidateYear, month - 1, day));
      if (
        date.getUTCFullYear() !== candidateYear ||
        date.getUTCMonth() !== month - 1 ||
        date.getUTCDate() !== day
      ) {
        return undefined;
      }
      const distance = Math.abs(
        Math.round((date.getTime() - reference.getTime()) / 86_400_000)
      );
      return { iso: isoDate(date), distance };
    })
    .filter(
      (
        candidate
      ): candidate is { iso: string; distance: number } =>
        candidate !== undefined &&
        (maximumDistanceDays === undefined ||
          candidate.distance <= maximumDistanceDays)
    )
    .sort((left, right) => left.distance - right.distance);
  return candidates[0]?.iso;
}

export function isoDate(value: Date): string {
  return [
    String(value.getUTCFullYear()).padStart(4, "0"),
    String(value.getUTCMonth() + 1).padStart(2, "0"),
    String(value.getUTCDate()).padStart(2, "0")
  ].join("-");
}
