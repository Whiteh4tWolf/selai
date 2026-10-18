import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { TranscriptWindow } from './TranscriptWindow';

it('bounds rendered history while retaining access to older and latest turns', () => {
  const items = Array.from({ length: 100 }, (_, i) => ({ id: `turn-${i}` }));
  const onBrowse = vi.fn();
  render(<ol><TranscriptWindow items={items} onBrowse={onBrowse} render={(item) => <li data-testid="turn">{item.id}</li>} /></ol>);
  expect(screen.getAllByTestId('turn')).toHaveLength(40);
  expect(screen.getByText('turn-99')).toBeInTheDocument();
  expect(screen.queryByText('turn-0')).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Earlier messages (60)' }));
  expect(onBrowse).toHaveBeenLastCalledWith(true);
  expect(screen.getByText('turn-20')).toBeInTheDocument();
  expect(screen.getAllByTestId('turn')).toHaveLength(40);
  fireEvent.click(screen.getByRole('button', { name: 'Earlier messages (20)' }));
  expect(screen.getByText('turn-0')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Latest messages' }));
  expect(onBrowse).toHaveBeenLastCalledWith(false);
  expect(screen.getByText('turn-99')).toBeInTheDocument();
  expect(items).toHaveLength(100);
});
