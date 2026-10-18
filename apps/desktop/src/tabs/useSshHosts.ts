import { useEffect, useState } from 'react';
import { hostList, HOSTS_CHANGED, describeError, type HostView } from '../ipc';
import { collectPages } from '../ui/collectPages';
export function useSshHosts(visible: boolean) {
  const [hosts, setHosts] = useState<HostView[]>([]);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!visible) return;
    let live = true;
    const load = () => {
      collectPages((cursor) => hostList(undefined, false, cursor))
        .then((rows) => {
          if (live) {
            setHosts(rows.filter((h) => h.kind === 'ssh'));
            setError(null);
          }
        })
        .catch((e) => {
          if (live) setError(describeError(e));
        });
    };
    load();
    window.addEventListener(HOSTS_CHANGED, load);
    return () => {
      live = false;
      window.removeEventListener(HOSTS_CHANGED, load);
    };
  }, [visible]);
  return { hosts, error };
}
