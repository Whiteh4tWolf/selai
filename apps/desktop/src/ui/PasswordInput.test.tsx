import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { Field } from './Field';
import { PasswordInput } from './PasswordInput';
import { flushVaultViews } from '../vault';

afterEach(() => vi.useRealTimers());

it('fetches a saved password only on demand and clears the preview when hidden', async () => {
  const loadSaved = vi.fn().mockResolvedValue('demo-saved-secret');
  const edit = vi.fn();
  render(<Field label="Password"><PasswordInput value="" onChange={edit} loadSaved={loadSaved} /></Field>);
  expect(loadSaved).not.toHaveBeenCalled();
  await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Show password' })));
  expect(screen.getByLabelText('Password')).toHaveValue('demo-saved-secret');
  expect(screen.getByLabelText('Password')).toHaveAttribute('type', 'text');
  expect(edit).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Hide password' }));
  expect(screen.getByLabelText('Password')).toHaveValue('');
  expect(screen.getByLabelText('Password')).toHaveAttribute('type', 'password');
});

it('auto-hides a saved password after 30 seconds and on window blur', async () => {
  vi.useFakeTimers();
  const loadSaved = vi.fn().mockResolvedValue('demo-secret');
  render(<PasswordInput id="password" value="" onChange={vi.fn()} loadSaved={loadSaved} />);
  await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Show password' })));
  act(() => vi.advanceTimersByTime(30_000));
  expect(screen.queryByDisplayValue('demo-secret')).not.toBeInTheDocument();
  await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Show password' })));
  fireEvent.blur(window);
  expect(screen.queryByDisplayValue('demo-secret')).not.toBeInTheDocument();
});

it('discards late reveal responses after an edit or loss of focus', async () => {
  let resolve: (value: string) => void = () => {};
  const loadSaved = () => new Promise<string>((done) => { resolve = done; });
  const edit = vi.fn();
  render(<Field label="Password"><PasswordInput value="" onChange={edit} loadSaved={loadSaved} /></Field>);
  fireEvent.click(screen.getByRole('button', { name: 'Show password' }));
  fireEvent.change(screen.getByLabelText('Password'), { target: { value: 'new-value' } });
  await act(async () => resolve('stale-secret'));
  expect(edit).toHaveBeenCalledWith('new-value');
  expect(screen.queryByDisplayValue('stale-secret')).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Show password' }));
  fireEvent.blur(window);
  await act(async () => resolve('stale-secret'));
  expect(screen.queryByDisplayValue('stale-secret')).not.toBeInTheDocument();
});

it('clears previews when vault locking flushes the still-mounted workspace', async () => {
  render(<Field label="Password"><PasswordInput value="" onChange={vi.fn()} loadSaved={vi.fn().mockResolvedValue('demo-secret')} /></Field>);
  await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Show password' })));
  expect(screen.getByLabelText('Password')).toHaveValue('demo-secret');
  await act(async () => { await flushVaultViews(); });
  expect(screen.getByLabelText('Password')).toHaveValue('');
  expect(screen.getByLabelText('Password')).toHaveAttribute('type', 'password');
});
