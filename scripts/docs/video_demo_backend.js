// Local recording fixture only. Never imported by the shipped application.
(() => {
  if (location.hostname !== '127.0.0.1') return;
  localStorage.setItem('selai.theme', 'dark');
  localStorage.setItem('selai.chat', 'closed');
  localStorage.setItem('selai.chat.width', '680');
  const hosts = [];
  const groups = [];
  let nextGroup = 1;
  const passwords = new Map();
  const sessions = new Map();
  const local = { id: 'local', label: 'This computer', hostname: 'localhost', username: 'demo', port: 22,
    authMethod: 'password', privateKeyPath: null, groupId: null, connectTimeoutSeconds: 15,
    keepAliveSeconds: 30, terminalType: 'xterm-256color', notes: null, credentialConfigured: false,
    updatedAtMs: 1, lastConnectedAtMs: null, archived: false, kind: 'local' };
  const provider = { id: 'demo-provider', name: 'Demo provider', protocol: 'openai_chat',
    baseUrl: 'https://api.example.com/v1', modelId: 'demo-tool-model', timeoutSeconds: 120,
    maxOutputTokens: 4096, keyConfigured: true, isLoopback: false, allowInsecureHttp: false,
    updatedAtMs: 1, toolCalling: { status: 'verified', lastCheckedAtMs: 1 } };
  const fingerprint = 'SHA256:V9m8wFq2Jc4t6sK0p3a5u7e9r1y4i6o8L2b0dXzNQhE';
  const terminal = new Map();
  let trusted = false;
  let agentChannel;
  let taskId = 'demo-task';
  let preferences = { lastProviderId: provider.id, pinnedHostId: null, outputRetentionDays: 30 };
  const emit = (event) => agentChannel?.onmessage({ taskId, ...event });
  const later = (delay, event) => setTimeout(() => emit(event), delay);
  const finish = () => {
    later(400, { type: 'state', state: 'running', errorCode: null });
    later(1600, { type: 'toolFinished', executionId: 'restart', state: 'completed', exitCode: 0, errorCode: null });
    later(2200, { type: 'toolProposed', executionId: 'verify', tool: 'diagnostic', command: 'systemctl --no-pager status -- nginx', hostLabel: 'staging-web' });
    later(3400, { type: 'toolFinished', executionId: 'verify', state: 'completed', exitCode: 0, errorCode: null });
    later(3900, { type: 'textDelta', text: '\n\n### Verification\n\n| Check | Result |\n| --- | --- |\n| nginx | **active (running)** |\n| Root filesystem | 31% used; 27 GB available |\n| Restart | exit code `0` |\n\nnginx is running again on **staging-web**. No other hosts were contacted.' });
    later(4600, { type: 'usage', inputTokens: 910, outputTokens: 170 });
    later(4900, { type: 'completed', text: '' });
  };
  window.__DEMO_RECORDING__ = { calls: [], hosts };
  window.isTauri = true;
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
    transformCallback: () => 1,
    unregisterCallback: () => {},
    invoke: async (command, args = {}) => {
      window.__DEMO_RECORDING__.calls.push(command);
      if (command.startsWith('plugin:')) return null;
      switch (command) {
        case 'vault_status': return { phase: 'unlocked', legacyFound: false, timeoutMinutes: 15,
          activeTasks: 0, terminals: terminal.size, cleanupPending: false, retryAfterMs: 0, flushRequired: false };
        case 'vault_activity': case 'vault_flush_complete': return null;
        case 'app_info': return { name: 'Selai', version: '0.1.1' };
        case 'storage_status': return { schemaVersion: 8, journalMode: 'memory', interruptedTasksAtStartup: 0, interruptedExecutionsAtStartup: 0 };
        case 'local_host': return local;
        case 'host_list': return { items: hosts, nextCursor: null };
        case 'host_get': return hosts.find(h => h.id === args.id);
        case 'host_move_group': {
          const host = hosts.find(h => h.id === args.id);
          if (!host || host.updatedAtMs !== args.expectedUpdatedAtMs) throw new Error('Profile changed. Refresh and try again.');
          host.groupId = args.groupId; host.updatedAtMs += 1; return { ...host };
        }
        case 'ssh_key_list': return [];
        case 'group_list': return groups;
        case 'group_create': {
          const group = { id: `demo-group-${nextGroup++}`, name: args.name, sortOrder: 0 };
          groups.push(group); return group;
        }
        case 'group_rename': {
          const group = groups.find(g => g.id === args.id); group.name = args.name; return group;
        }
        case 'group_delete': {
          const index = groups.findIndex(g => g.id === args.id);
          if (index >= 0) groups.splice(index, 1);
          for (const host of hosts) if (host.groupId === args.id) host.groupId = null;
          return null;
        }
        case 'host_reveal_password': return passwords.get(args.id) ?? null;
        case 'host_save': {
          const { password, keyPassphrase, expectedUpdatedAtMs, ...fields } = args.input;
          const host = { ...local, ...fields, id: fields.id ?? (hosts.length ? crypto.randomUUID() : 'staging-web-id'), credentialConfigured: true, updatedAtMs: Date.now(), kind: 'ssh' };
          const index = hosts.findIndex(h => h.id === host.id);
          if (password) passwords.set(host.id, password);
          if (index < 0) hosts.push(host); else hosts[index] = host;
          return host;
        }
        case 'terminal_connect': {
          if (!trusted) throw { code: 'SSH_HOSTKEY_UNKNOWN', message: 'Server key is not trusted yet',
            details: { algorithm: 'ssh-ed25519', fingerprintSha256: fingerprint } };
          const channel = args.onEvent;
          const id = 'demo-terminal'; terminal.set(id, { channel, input: '' });
          setTimeout(() => channel.onmessage({ type: 'data', data: btoa('\x1b[32mConnected to staging-web\x1b[0m (192.0.2.42:22)\r\nUbuntu 24.04 LTS | OpenSSH | deployment user: deploy\r\n\r\ndeploy@staging-web:~$ ') }), 250);
          return id;
        }
        case 'host_trust': trusted = true; return { outcome: 'pinned', fingerprintSha256: fingerprint };
        case 'terminal_resize': return null;
        case 'terminal_disconnect': terminal.delete(args.sessionId); return null;
        case 'terminal_input': {
          const state = terminal.get(args.sessionId);
          if (!state) return null;
          state.channel.onmessage({ type: 'data', data: btoa(args.data.replaceAll('\r', '\r\n')) });
          state.input += args.data;
          if (args.data.includes('\r')) {
            if (state.input.trim() === 'exit') {
              state.channel.onmessage({ type: 'data', data: btoa('\r\nlogout\r\n') });
              state.channel.onmessage({ type: 'exit', status: 0 });
              state.channel.onmessage({ type: 'closed', exitStatus: 0 });
              terminal.delete(args.sessionId); return null;
            }
            state.channel.onmessage({ type: 'data', data: btoa('staging-web\r\n 10:42:18 up 3 days, 2:15, 1 user, load average: 0.02, 0.03, 0.01\r\n\r\ndeploy@staging-web:~$ ') });
            state.input = '';
          }
          return null;
        }
        case 'provider_list': return { items: [provider], nextCursor: null };
        case 'provider_templates': return [];
        case 'preferences_get': return preferences;
        case 'preferences_set': preferences = { ...preferences, ...args.input }; return preferences;
        case 'chat_session_list': return [...sessions.values()].map(({ transcript, ...summary }) => summary);
        case 'chat_session_save': {
          const prev = sessions.get(args.input.id);
          sessions.set(args.input.id, { ...args.input, createdAtMs: prev?.createdAtMs ?? Date.now(), updatedAtMs: Date.now() }); return null;
        }
        case 'chat_session_get': return sessions.get(args.id);
        case 'agent_start': {
          agentChannel = args.onEvent;
          later(300, { type: 'state', state: 'running', errorCode: null });
          later(650, { type: 'textDelta', text: 'I will inspect disk space and nginx on **staging-web** before making a change.\n\n' });
          later(1100, { type: 'toolProposed', executionId: 'disk', tool: 'diagnostic', command: 'df -hP /', hostLabel: 'staging-web' });
          later(1800, { type: 'toolFinished', executionId: 'disk', state: 'completed', exitCode: 0, errorCode: null });
          later(2300, { type: 'toolProposed', executionId: 'status', tool: 'diagnostic', command: 'systemctl --no-pager status -- nginx', hostLabel: 'staging-web' });
          later(3100, { type: 'toolFinished', executionId: 'status', state: 'completed', exitCode: 3, errorCode: null });
          later(3500, { type: 'textDelta', text: 'The root filesystem is 31% used. nginx is **inactive (dead)**. I propose restarting the service, then checking its status again.' });
          later(4200, { type: 'toolProposed', executionId: 'restart', tool: 'command', command: 'sudo systemctl restart nginx', hostLabel: 'staging-web' });
          later(4700, { type: 'approvalRequired', approvalId: 'demo-approval', executionId: 'restart', command: 'sudo systemctl restart nginx', expiresAtMs: Date.now() + 300000, sudo: 'no_password' });
          return taskId;
        }
        case 'approval_decide':
          if (args.approve) finish();
          else emit({ type: 'completed', text: 'Restart denied. No change was made.' });
          return null;
        case 'sudo_options': return { hostLabel: 'staging-web', savedPassword: false, loginPassword: true };
        case 'agent_cancel': return true;
        default: throw new Error(`Recording fixture does not implement ${command}`);
      }
    },
  };
})();
