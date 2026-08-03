export function normalizeHoursRange(value: string): string | undefined {
  const match = value.match(
    /(\d{1,2})[.:](\d{2})\s*[-–—]\s*(\d{1,2})[.:](\d{2})/
  );
  if (!match) return undefined;

  const startHour = Number(match[1]);
  const startMinute = Number(match[2]);
  const endHour = Number(match[3]);
  const endMinute = Number(match[4]);
  if (
    startHour > 23 ||
    endHour > 23 ||
    startMinute > 59 ||
    endMinute > 59 ||
    startHour * 60 + startMinute >= endHour * 60 + endMinute
  ) {
    return undefined;
  }

  const time = (hour: number, minute: number) =>
    `${String(hour).padStart(2, "0")}:${String(minute).padStart(2, "0")}`;
  return `${time(startHour, startMinute)}–${time(endHour, endMinute)}`;
}
