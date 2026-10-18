# SSH profile and AI Assistant demo

[Animated README preview](selai-ssh-ai-demo.gif) ·
[Download the MP4](selai-ssh-ai-demo.mp4?raw=true) ·
[Poster](selai-ssh-ai-demo-poster.jpg) ·
[English subtitles](selai-ssh-ai-demo.en.srt)

This video is about 59 seconds, 1600 × 900, H.264/yuv420p, with English captions
and no audio. The application fills the frame in dark mode. There are no
presentation cards or colored background around the workspace. The small
caption band identifies the current action and labels the mock backend.

The main project README displays a looping GIF in place of the top screenshot.
The preview is 960 × 540 at 6 fps with 96 colors; the MP4 retains the original
resolution and frame rate. Animated playback follows the viewer's GitHub
accessibility settings.

## Workflow

1. Add `staging-web` with address `192.0.2.42`, port `22`, deployment user
   `deploy`, and password authentication.
2. Save and connect, inspect the Ed25519 SHA-256 server fingerprint, then trust it.
3. Open the terminal and run `hostname; uptime`.
4. Open AI Assistant, name the session, and select `staging-web` through `@`.
5. Ask it to check nginx and disk space, restarting nginx only if stopped.
6. Inspect the diagnostics and proposed `sudo systemctl restart nginx` command.
7. Approve that command and inspect the service verification and disk summary.

## Recording scope

This records real clicks, typing, dialogs, terminal rendering, and AI transcript
updates in Selai's production frontend bundle. SSH and AI responses are fixtures
from a recording-only mock IPC backend. No server is contacted, no shell command
is executed, and no real provider key, vault, or saved host is used.

The AI provider is preconfigured and marked verified in the fixture so the video
can focus on SSH profile creation and assistant use. The demo's sudo fixture does
not ask for a password. Real hosts can prompt for one. The optional task root
approval checkbox is left unchecked in the recording.

## Re-record

Build the frontend, then run the isolated recording harness from the repository root:

```sh
pnpm --dir apps/desktop build
python -m pip install playwright imageio-ffmpeg
python -m playwright install chromium
python scripts/docs/record_quickstart.py
python scripts/docs/render_readme_gif.py
```

The harness starts a temporary Vite preview server on loopback port 1426 and a
headless Chromium recording context. It blocks requests outside that local
origin and uses `scripts/docs/video_demo_backend.js` only in the recording.
Neither the fixture nor the recording shell is included in the application bundle.

Generated MP4, GIF, poster, and English SRT files are written here. Raw recordings,
review frames, and command-name checks are written under `target/video-qa/technical`.
