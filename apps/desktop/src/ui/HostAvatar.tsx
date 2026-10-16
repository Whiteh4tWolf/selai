import { ServerIcon } from './icons';

const PALETTE = ['#5b7cfa', '#2fbf71', '#e8a23a', '#e5604f', '#9b6bff', '#1fb2c9', '#e0683a', '#d14d8a'];

/** A stable colour per label, so a host is recognisable at a glance. */
export function colorFor(label: string): string {
  let hash = 0;
  for (const ch of label.toLowerCase()) hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
  return PALETTE[hash % PALETTE.length] ?? PALETTE[0]!;
}

export function HostAvatar({ label, size = 18 }: { label: string; size?: number }) {
  return (
    <span className="avatar" style={{ background: colorFor(label) }}>
      <ServerIcon size={size} />
    </span>
  );
}
