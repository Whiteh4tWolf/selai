// Number and time formatting shared by the views. Fixed to en-US digits so the
// same count reads the same everywhere in the app.

const exact = new Intl.NumberFormat('en-US');

/** 12345 -> "12,345". `null` (not reported) -> "—". */
export function formatCount(n: number | null | undefined): string {
  return n === null || n === undefined ? '—' : exact.format(n);
}

/** 12345 -> "12.3k", 2500000 -> "2.5M". Small numbers stay exact. */
export function formatCompact(n: number | null | undefined): string {
  if (n === null || n === undefined) return '—';
  if (n < 1000) return String(n);
  const [value, unit] = n < 1_000_000 ? [n / 1000, 'k'] : [n / 1_000_000, 'M'];
  return `${value >= 100 ? Math.round(value) : value.toFixed(1).replace(/\.0$/, '')}${unit}`;
}

/** Sum of two counts where either may be unknown. Unknown only if both are. */
export function sumTokens(a: number | null, b: number | null): number | null {
  return a === null && b === null ? null : (a ?? 0) + (b ?? 0);
}

export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  return `${m}m ${s % 60}s`;
}

export function formatWhen(ms: number): string {
  return new Date(ms).toLocaleString();
}
