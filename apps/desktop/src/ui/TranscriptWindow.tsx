import { Fragment, useState, type ReactNode } from 'react';

const WINDOW = 40;

/** Keep DOM/Markdown bounded while keeping every saved turn accessible. */
export function TranscriptWindow<T extends { id: string }>({ items, render, onBrowse }: {
  items: T[];
  render: (item: T) => ReactNode;
  onBrowse?: (older: boolean) => void;
}) {
  const [position, setPosition] = useState<number | null>(null);
  const latest = Math.max(0, items.length - WINDOW);
  const start = position === null ? latest : Math.min(position, latest);
  const navigate = (next: number | null) => { setPosition(next); onBrowse?.(next !== null); };
  return <>
    {start > 0 && <li className="transcript-navigation"><button type="button" className="btn btn-sm" onClick={() => navigate(Math.max(0, start - WINDOW))}>Earlier messages ({start})</button></li>}
    {items.slice(start, start + WINDOW).map((item) => <Fragment key={item.id}>{render(item)}</Fragment>)}
    {position !== null && <li className="transcript-navigation">
      {start + WINDOW < items.length && <button type="button" className="btn btn-sm" onClick={() => navigate(start + WINDOW)}>Next messages</button>}
      <button type="button" className="btn btn-sm" onClick={() => navigate(null)}>Latest messages</button>
    </li>}
  </>;
}
