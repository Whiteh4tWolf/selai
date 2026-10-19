# Selai feature walkthrough

[Play the walkthrough](https://github.com/user-attachments/assets/2e0f8ef4-4df9-4208-8b55-97e4cbb33f00) ·
[Download the MP4](selai-ssh-ai-demo.mp4?raw=true) ·
[Poster](selai-ssh-ai-demo-poster.jpg) ·
[English subtitles](selai-ssh-ai-demo.en.srt)

The walkthrough covers **SSH Profile**, **AI Assistant**, **Port forwarding**,
and **Proxies** using Selai's current production frontend. The 104-second recording fills
1600 × 900 frames in dark mode, with a small English caption band and no audio.
The MP4 uses H.264/yuv420p with fast-start playback. The project README embeds
the MP4 as a GitHub attachment, providing native play/pause, seeking, and
fullscreen controls. The GIF is retained as an optional
fallback asset and is no longer embedded in the README.

## Workflows

1. **SSH Profile:** create `staging-web` in the Staging group, enter the sample
   address `192.0.2.42`, port `22`, deployment username, and password. Save and
   connect, inspect the Ed25519 fingerprint, trust it, and run `hostname; uptime`
   in the terminal. Reopen the saved profile to show the editor.
2. **AI Assistant:** name a session, choose the preconfigured provider and its
   server-ready model, and target `staging-web` with `@`. Ask for nginx and disk
   diagnostics, inspect the proposed `sudo systemctl restart nginx`, approve
   that command, and review the verification and disk summary.
3. **Port forwarding:** save and start a local PostgreSQL tunnel from
   `127.0.0.1:15432` to `db.internal:5432`. Inspect its connection and traffic
   counters. Save a remote webhook tunnel and a dynamic SOCKS5 tunnel to show
   all three forwarding modes.
4. **Proxies:** save an authenticated SOCKS5 office gateway and an HTTP CONNECT
   corporate gateway. Edit the SSH profile, enable proxy routing, select the
   office gateway, and save the profile.

## Recording scope

The recording uses real clicks, typing, dialogs, terminal rendering, and AI
transcript updates. SSH, AI, proxy, and tunnel responses come from a
recording-only mock IPC backend. Traffic counters are fixture values, not
measurements. No server is contacted, no shell command or network tunnel is
started, and no real provider key, vault, or saved host is used.

The AI provider is preconfigured and marked tool-calling verified. The sudo
fixture does not ask for a password; real hosts can prompt for one. The optional
root approval for the rest of the task remains unchecked.

## Re-record

From the repository root, build the frontend and run the isolated harness:

```sh
pnpm --dir apps/desktop build
python -m pip install playwright imageio-ffmpeg
python -m playwright install chromium
python scripts/docs/record_quickstart.py
```

The harness starts a temporary Vite preview server on loopback port 1426 and a
fresh headless Chromium context. Requests outside that origin are blocked.
`scripts/docs/video_demo_backend.js` is loaded only into this recording context;
it is not imported by the application bundle.

Generated MP4, poster, and English SRT files are written here. After re-recording,
upload the MP4 through GitHub's Markdown editor and update the attachment links
in both READMEs. Keep the `https://github.com/user-attachments/assets/...` URL
on its own line in the project README so GitHub renders a player. A repository file link alone does not produce the native player.
To regenerate the optional GIF fallback, run `python scripts/docs/render_readme_gif.py`.

The README's four primary screenshots are captured from the app without the caption band:

- `docs/screenshots/ssh-profile.png`
- `docs/screenshots/ai-assistant.png`
- `docs/screenshots/port-forwarding.png`
- `docs/screenshots/proxies.png`

Additional captures show the SSH profile's selected proxy, connected workspace,
and AI approval. Raw video and recording checks are saved under
`target/video-qa/technical`. The harness checks for all four workflows' IPC calls,
three tunnel modes, two proxy protocols, an assigned proxy, and frontend errors.
