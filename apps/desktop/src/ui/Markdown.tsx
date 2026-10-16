import { lazy, Suspense } from 'react';

// Markdown and Mermaid stay out of the startup bundle until a reply needs them.
const MarkdownContent = lazy(() => import('./MarkdownContent'));

export function Markdown({ text }: { text: string }) {
  return (
    <Suspense fallback={<div className="md md-pending">{text}</div>}>
      <MarkdownContent text={text} />
    </Suspense>
  );
}
