import { describe, expect, it, vi } from 'vitest';
import { collectPages } from './collectPages';

describe('collectPages', () => {
  it('finds hosts beyond the backend first-page limit', async () => {
    const load = vi.fn(async (cursor?: string) => cursor
      ? { items: ['host-51'], nextCursor: null }
      : { items: Array.from({ length: 50 }, (_, index) => `host-${index + 1}`), nextCursor: 'next' });
    const hosts = await collectPages(load);
    expect(hosts).toHaveLength(51);
    expect(hosts[50]).toBe('host-51');
    expect(load).toHaveBeenNthCalledWith(2, 'next');
  });

  it('stops on a repeated cursor instead of looping forever', async () => {
    await expect(collectPages(async () => ({ items: [], nextCursor: 'same' }))).rejects.toThrow('did not advance');
  });
});
