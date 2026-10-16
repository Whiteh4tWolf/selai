import { useState, type ReactNode } from 'react';
import { CheckIcon } from './icons';

/**
 * A small Markdown renderer for model replies: fenced code, inline code, bold,
 * headings, lists, quotes and paragraphs. It builds React elements only (no
 * HTML strings), so model text can never inject markup. Links are shown as text
 * with their address: the webview must not navigate away on a click.
 */
export function Markdown({ text }: { text: string }) {
  return <div className="md">{blocks(text)}</div>;
}

function blocks(text: string): ReactNode[] {
  const lines = text.replace(/\r\n/g, '\n').split('\n');
  const out: ReactNode[] = [];
  let i = 0;
  let key = 0;
  while (i < lines.length) {
    const line = lines[i] ?? '';
    const fence = /^\s*```\s*([\w+-]*)\s*$/.exec(line);
    if (fence) {
      const body: string[] = [];
      i += 1;
      // An unclosed fence (still streaming) runs to the end.
      while (i < lines.length && !/^\s*```\s*$/.test(lines[i] ?? '')) {
        body.push(lines[i] ?? '');
        i += 1;
      }
      i += 1;
      out.push(<CodeBlock key={key++} lang={fence[1] ?? ''} code={body.join('\n')} />);
      continue;
    }
    if (!line.trim()) {
      i += 1;
      continue;
    }
    const heading = /^(#{1,4})\s+(.*)$/.exec(line);
    if (heading) {
      const level = heading[1]?.length ?? 1;
      const content = inline(heading[2] ?? '');
      out.push(
        level <= 2 ? <h3 key={key++}>{content}</h3> : <h4 key={key++}>{content}</h4>,
      );
      i += 1;
      continue;
    }
    if (/^\s*([-*+]|\d+[.)])\s+/.test(line)) {
      const ordered = /^\s*\d+[.)]\s+/.test(line);
      const items: ReactNode[] = [];
      while (i < lines.length && /^\s*([-*+]|\d+[.)])\s+/.test(lines[i] ?? '')) {
        items.push(<li key={items.length}>{inline((lines[i] ?? '').replace(/^\s*([-*+]|\d+[.)])\s+/, ''))}</li>);
        i += 1;
      }
      out.push(ordered ? <ol key={key++}>{items}</ol> : <ul key={key++}>{items}</ul>);
      continue;
    }
    if (/^\s*>/.test(line)) {
      const quoted: string[] = [];
      while (i < lines.length && /^\s*>/.test(lines[i] ?? '')) {
        quoted.push((lines[i] ?? '').replace(/^\s*>\s?/, ''));
        i += 1;
      }
      out.push(<blockquote key={key++}>{inline(quoted.join(' '))}</blockquote>);
      continue;
    }
    const para: string[] = [];
    while (
      i < lines.length &&
      (lines[i] ?? '').trim() &&
      !/^\s*```/.test(lines[i] ?? '') &&
      !/^(#{1,4})\s+/.test(lines[i] ?? '') &&
      !/^\s*([-*+]|\d+[.)])\s+/.test(lines[i] ?? '') &&
      !/^\s*>/.test(lines[i] ?? '')
    ) {
      para.push(lines[i] ?? '');
      i += 1;
    }
    out.push(<p key={key++}>{inline(para.join('\n'))}</p>);
  }
  return out;
}

/** `code`, **bold** and [text](url). Everything else is plain text. */
function inline(text: string): ReactNode[] {
  const out: ReactNode[] = [];
  const pattern = /(`[^`\n]+`)|(\*\*[^*\n]+\*\*)|(\[[^\]\n]+\]\([^)\s]+\))/g;
  let last = 0;
  let key = 0;
  for (const m of text.matchAll(pattern)) {
    const at = m.index ?? 0;
    if (at > last) out.push(text.slice(last, at));
    const token = m[0];
    if (token.startsWith('`')) {
      out.push(<code key={key++}>{token.slice(1, -1)}</code>);
    } else if (token.startsWith('**')) {
      out.push(<strong key={key++}>{token.slice(2, -2)}</strong>);
    } else {
      const link = /^\[([^\]]+)\]\(([^)]+)\)$/.exec(token);
      out.push(
        <span key={key++} className="md-link" title={link?.[2]}>
          {link?.[1]} <span className="muted">({link?.[2]})</span>
        </span>,
      );
    }
    last = at + token.length;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

function CodeBlock({ lang, code }: { lang: string; code: string }) {
  const [copied, setCopied] = useState(false);
  async function copy() {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      // Clipboard access can be refused; the text is still selectable.
    }
  }
  return (
    <div className="md-code">
      <div className="md-code-head">
        <span>{lang || 'code'}</span>
        <button type="button" className="btn btn-ghost btn-sm" onClick={() => void copy()}>
          {copied ? (
            <>
              <CheckIcon size={12} /> Copied
            </>
          ) : (
            'Copy'
          )}
        </button>
      </div>
      <pre>
        <code>{code}</code>
      </pre>
    </div>
  );
}
