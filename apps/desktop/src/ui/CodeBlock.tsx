import { useState } from 'react';
import { CheckIcon } from './icons';

export function CodeBlock({ lang, code }: { lang: string; code: string }) {
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
          {copied ? <><CheckIcon size={12} /> Copied</> : 'Copy'}
        </button>
      </div>
      <pre><code>{code}</code></pre>
    </div>
  );
}
