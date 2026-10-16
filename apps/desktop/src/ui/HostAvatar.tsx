import { ServerIcon } from './icons';

const PALETTE = ['#4c6460', '#57654c', '#756349', '#76584f', '#625b74', '#47676d', '#6d6257', '#705b65'];

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
