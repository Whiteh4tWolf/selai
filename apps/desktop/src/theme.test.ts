import { act, renderHook } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { useResolvedTheme, type Theme } from './theme';

afterEach(() => vi.unstubAllGlobals());

it('follows system changes only when System is selected and removes the listener', () => {
  let changed = () => {};
  const media = { matches: false, addEventListener: vi.fn((_name, callback) => { changed = callback; }), removeEventListener: vi.fn() };
  vi.stubGlobal('matchMedia', () => media);
  const { result, rerender, unmount } = renderHook(({ theme }: { theme: Theme }) => useResolvedTheme(theme), { initialProps: { theme: 'system' as Theme } });
  expect(result.current).toBe('dark');
  act(() => { media.matches = true; changed(); });
  expect(result.current).toBe('light');
  rerender({ theme: 'dark' });
  expect(result.current).toBe('dark');
  unmount();
  expect(media.removeEventListener).toHaveBeenCalledWith('change', changed);
});
