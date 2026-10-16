import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import MermaidDiagram from './MermaidDiagram';

const parse = vi.fn();
const renderDiagram = vi.fn();
const initialize = vi.fn();
vi.mock('mermaid', () => ({ default: { initialize, parse, render: renderDiagram } }));

beforeEach(() => {
  vi.clearAllMocks();
  parse.mockResolvedValue({ diagramType: 'flowchart' });
  renderDiagram.mockResolvedValue({ svg: '<svg xmlns="http://www.w3.org/2000/svg"><text>OK</text></svg>' });
});

describe('MermaidDiagram', () => {
  it('renders a settled diagram as an isolated SVG image', async () => {
    render(<MermaidDiagram code={'flowchart LR\nA --> B'} />);
    expect(screen.getByText('mermaid').closest('.md-code')).toHaveTextContent('flowchart LR A --> B');
    const image = await screen.findByRole('img', { name: 'Mermaid diagram' });
    expect(image.getAttribute('src')).toMatch(/^data:image\/svg\+xml;charset=utf-8,/);
    expect(parse).toHaveBeenCalledWith('flowchart LR\nA --> B', { suppressErrors: true });
    expect(initialize).toHaveBeenCalledWith(expect.objectContaining({ securityLevel: 'strict', startOnLoad: false }));
  });

  it('keeps source visible when syntax is invalid', async () => {
    parse.mockResolvedValue(false);
    render(<MermaidDiagram code="broken diagram" />);
    expect(await screen.findByText('Diagram could not be rendered; source is shown below.')).toBeInTheDocument();
    expect(screen.getByText('broken diagram')).toBeInTheDocument();
    expect(renderDiagram).not.toHaveBeenCalled();
  });
});
