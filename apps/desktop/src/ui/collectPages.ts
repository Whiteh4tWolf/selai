import type { Page } from '../ipc';

/** Host lists are paged at 50 records by the backend. */
export async function collectPages<T>(load: (cursor?: string) => Promise<Page<T>>): Promise<T[]> {
  const items: T[] = [];
  let cursor: string | undefined;
  for (let pageNumber = 0; pageNumber < 200; pageNumber += 1) {
    const page = await load(cursor);
    items.push(...page.items);
    if (!page.nextCursor) return items;
    if (page.nextCursor === cursor) throw new Error('Page cursor did not advance.');
    cursor = page.nextCursor;
  }
  throw new Error('Too many pages to load.');
}
