import { ProvidersPanel } from './ProvidersPanel';

export function AiTab() {
  return (
    <section aria-label="AI">
      <h2>AI</h2>
      <p>
        The agent is not available yet (milestones M3 and M4). Providers can be configured and
        tested here. No command can run from this screen.
      </p>
      <ProvidersPanel />
    </section>
  );
}
