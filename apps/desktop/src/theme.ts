import { useEffect, useState } from 'react';

export type Theme = 'dark' | 'light' | 'system';
export type ResolvedTheme = Exclude<Theme, 'system'>;

/** One system-theme subscription for the workbench, shared by all terminals. */
export function useResolvedTheme(theme: Theme): ResolvedTheme {
  const [systemLight, setSystemLight] = useState(() => window.matchMedia?.('(prefers-color-scheme: light)').matches ?? false);
  useEffect(() => {
    const media = window.matchMedia?.('(prefers-color-scheme: light)');
    if (!media) return;
    const changed = () => setSystemLight(media.matches);
    changed();
    media.addEventListener('change', changed);
    return () => media.removeEventListener('change', changed);
  }, []);
  return theme === 'system' ? (systemLight ? 'light' : 'dark') : theme;
}
