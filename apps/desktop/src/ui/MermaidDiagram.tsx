import { useEffect, useId, useState } from 'react';
import { CodeBlock } from './CodeBlock';

let mermaidLoader: Promise<(typeof import('mermaid'))['default']> | null = null;

function loadMermaid() {
  mermaidLoader ??= import('mermaid').then(({ default: mermaid }) => {
    mermaid.initialize({
      startOnLoad: false,
      securityLevel: 'strict',
      suppressErrorRendering: true,
      maxTextSize: 20_000,
      theme: 'dark',
      flowchart: { htmlLabels: false },
    });
    return mermaid;
  });
  return mermaidLoader;
}

export default function MermaidDiagram({ code }: { code: string }) {
  const id = `selai-diagram-${useId().replace(/[^a-zA-Z0-9]/g, '')}`;
  const [rendered, setRendered] = useState<{ code: string; src: string } | null>(null);
  const [invalid, setInvalid] = useState<string | null>(null);

  useEffect(() => {
    if (!code.trim()) return;
    let active = true;
    // A fenced block may still be streaming. Wait until it settles so Mermaid
    // does not parse and redraw on every model token.
    const timer = window.setTimeout(async () => {
      try {
        if (code.length > 20_000) throw new Error('Diagram is too large.');
        const mermaid = await loadMermaid();
        const valid = await mermaid.parse(code, { suppressErrors: true });
        if (!valid) throw new Error('Invalid Mermaid syntax.');
        const { svg } = await mermaid.render(id, code);
        if (active) {
          setRendered({ code, src: `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}` });
          setInvalid(null);
        }
      } catch {
        if (active) setInvalid(code);
      }
    }, 300);
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [code, id]);

  if (rendered?.code === code) {
    return <figure className="md-diagram"><img src={rendered.src} alt="Mermaid diagram" /><figcaption>Mermaid diagram</figcaption></figure>;
  }
  return <div>{invalid === code && <p className="md-diagram-error">Diagram could not be rendered; source is shown below.</p>}<CodeBlock lang="mermaid" code={code} /></div>;
}
