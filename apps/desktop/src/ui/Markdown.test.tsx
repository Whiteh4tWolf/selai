import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import MarkdownContent from './MarkdownContent';

describe('Markdown', () => {
  it('renders GFM tables, task lists, emphasis, code, and safe link text', async () => {
    render(<MarkdownContent text={'# Report\n\n**Healthy** and *ready*.\n\n| Host | Status |\n| --- | --- |\n| web | up |\n\n- [x] Checked\n- [ ] Pending\n\n```bash\nuptime\n```\n\n[Docs](https://example.com)'} />);
    expect(await screen.findByRole('heading', { name: 'Report' })).toBeInTheDocument();
    expect(screen.getByText('Healthy').tagName).toBe('STRONG');
    expect(screen.getByRole('table')).toHaveTextContent('webup');
    expect(screen.getAllByRole('checkbox')).toHaveLength(2);
    expect(screen.getByText('uptime')).toBeInTheDocument();
    expect(screen.getByText('bash')).toBeInTheDocument();
    expect(screen.queryByRole('link')).not.toBeInTheDocument();
    expect(screen.getByText(/https:\/\/example.com/)).toBeInTheDocument();
  });

  it('ignores raw HTML and never loads an AI-provided remote image', async () => {
    render(<MarkdownContent text={'<script>alert(1)</script>\n\n![private](https://example.com/track.png)\n\n[unsafe](javascript:alert(1))'} />);
    expect(await screen.findByText('private')).toBeInTheDocument();
    expect(screen.queryByText('alert(1)')).not.toBeInTheDocument();
    expect(screen.queryByRole('img')).not.toBeInTheDocument();
    expect(screen.queryByRole('link')).not.toBeInTheDocument();
  });
});
