import { Children, isValidElement, lazy, Suspense, type ReactNode } from 'react';
import ReactMarkdown, { type Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { CodeBlock } from './CodeBlock';

const MermaidDiagram = lazy(() => import('./MermaidDiagram'));

function codeParts(children: ReactNode): { code: string; lang: string } {
  const child = Children.toArray(children)[0];
  if (!isValidElement<{ className?: string; children?: ReactNode }>(child)) {
    return { code: '', lang: '' };
  }
  const lang = /(?:^|\s)language-([\w+-]+)/.exec(child.props.className ?? '')?.[1] ?? '';
  const code = Children.toArray(child.props.children).map((part) => typeof part === 'string' ? part : '').join('').replace(/\n$/, '');
  return { code, lang };
}

const components: Components = {
  a({ href, children }) {
    // AI-provided links are readable and copyable, but never navigate the webview.
    return <span className="md-link" title={href}>{children}{href && <span className="muted"> ({href})</span>}</span>;
  },
  img({ alt }) {
    // Do not fetch remote images supplied by an AI response.
    return <span className="md-image-alt">{alt || 'Image'}</span>;
  },
  table({ children }) {
    return <div className="md-table"><table>{children}</table></div>;
  },
  pre({ children }) {
    const { code, lang } = codeParts(children);
    if (lang.toLowerCase() === 'mermaid') {
      return <Suspense fallback={<CodeBlock lang={lang} code={code} />}><MermaidDiagram code={code} /></Suspense>;
    }
    return <CodeBlock lang={lang} code={code} />;
  },
};

export default function MarkdownContent({ text }: { text: string }) {
  return (
    <div className="md">
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={components} skipHtml>{text}</ReactMarkdown>
    </div>
  );
}
