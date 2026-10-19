"""Record real Selai frontend interactions against an isolated mock IPC backend.

No native vault, SSH server, or AI provider is contacted. The recording harness
is separate from the application bundle. Requires playwright and imageio-ffmpeg.
"""
from pathlib import Path
import json
import os
import re
import subprocess
import time
import urllib.request

import imageio_ffmpeg
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "docs/media"
SCREENSHOTS = ROOT / "docs/screenshots"
WORK = ROOT / "target/video-qa/technical"
URL = "http://127.0.0.1:1426"

SHELL = """<!doctype html><html><head><meta charset="UTF-8"><style>
html,body{margin:0;width:100%;height:100%;background:#111315;overflow:hidden;font-family:'Segoe UI',Arial,sans-serif}
iframe{display:block;border:0;width:100%;height:calc(100vh - 72px)}
#caption{height:72px;box-sizing:border-box;border-top:1px solid #34383c;color:#e7e9ec;display:flex;align-items:center;padding:0 26px;gap:22px;font-size:20px}
#step{color:#b1b8c0;font:600 15px 'Segoe UI',sans-serif;min-width:120px;letter-spacing:.07em}
#text{flex:1}#badge{color:#8c959f;font-size:12px;letter-spacing:.06em;white-space:nowrap}
#cursor{position:fixed;pointer-events:none;z-index:999999;width:24px;height:28px;left:60px;top:90px;filter:drop-shadow(0 1px 3px #000)}
#ring{position:fixed;pointer-events:none;z-index:999998;width:36px;height:36px;border:2px solid #ced8e2;border-radius:50%;opacity:0;transform:translate(-50%,-50%)}
</style></head><body><iframe id="app-frame" src="http://127.0.0.1:1426"></iframe>
<div id="caption"><span id="step">SSH PROFILE</span><span id="text">SSH profiles, AI assistance, port forwarding, and reusable proxies.</span><span id="badge">DEMO · SIMULATED BACKEND</span></div>
<svg id="cursor" viewBox="0 0 24 28"><path d="M2 1 2 23 8 17 12 26 16 24 12 15 21 15Z" fill="#f5f7fa" stroke="#111315" stroke-width="1.4"/></svg><div id="ring"></div>
</body></html>"""


def timestamp(seconds):
    ms = round(seconds * 1000)
    return f"{ms//3600000:02}:{ms//60000%60:02}:{ms//1000%60:02},{ms%1000:03}"


def main():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    SCREENSHOTS.mkdir(parents=True, exist_ok=True)
    WORK.mkdir(parents=True, exist_ok=True)
    log = (WORK / "vite.log").open("w", encoding="utf-8")
    args = ["pnpm", "exec", "vite", "preview", "--host", "127.0.0.1", "--port", "1426", "--strictPort"]
    flags = {}
    if os.name == "nt":
        args = ["cmd.exe", "/d", "/c", "pnpm.cmd", *args[1:]]
        flags["creationflags"] = subprocess.CREATE_NO_WINDOW
    server = subprocess.Popen(args, cwd=ROOT / "apps/desktop", stdout=log, stderr=log, **flags)
    cues, errors = [], []
    try:
        for _ in range(100):
            try:
                urllib.request.urlopen(URL, timeout=1).close()
                break
            except OSError:
                time.sleep(.2)
        else:
            raise RuntimeError("Recording dev server did not start; see target/video-qa/technical/vite.log")
        with sync_playwright() as p:
            browser = p.chromium.launch(headless=True)
            context = browser.new_context(viewport={"width":1600,"height":900}, color_scheme="dark",
                record_video_dir=str(WORK), record_video_size={"width":1600,"height":900})
            def route_request(route):
                if route.request.url == URL + "/__recording-shell__":
                    route.fulfill(status=200, content_type="text/html", body=SHELL)
                elif route.request.url.startswith(URL):
                    route.continue_()
                else:
                    route.abort()
            context.route("**/*", route_request)
            context.add_init_script(path=str(ROOT / "scripts/docs/video_demo_backend.js"))
            page = context.new_page()
            page.set_default_timeout(15000)
            recording_begin = time.monotonic()
            page.on("pageerror", lambda error: errors.append(str(error)))
            page.goto(URL + "/__recording-shell__")
            app = page.frame_locator("#app-frame")
            app.get_by_role("button", name="New host", exact=True).first.wait_for()
            started = time.monotonic()
            lead = started - recording_begin

            def cue(step, text):
                cues.append((time.monotonic()-started, step, text))
                page.evaluate("([step,text])=>{document.querySelector('#step').textContent=step;document.querySelector('#text').textContent=text}", [step,text])
                print(f"{time.monotonic()-started:5.1f}s {step}: {text}", flush=True)

            def click(locator):
                locator.wait_for(state="visible")
                box = locator.bounding_box()
                if not box: raise RuntimeError("Recording target has no bounds")
                x, y = box["x"]+box["width"]/2, box["y"]+box["height"]/2
                page.mouse.move(x,y,steps=20)
                page.evaluate("([x,y])=>{for(const id of ['cursor','ring']){const el=document.getElementById(id);el.style.left=x+'px';el.style.top=y+'px'}document.getElementById('ring').animate([{opacity:.8,scale:.8},{opacity:0,scale:1.6}],{duration:650})}",[x,y])
                page.wait_for_timeout(250)
                locator.click()

            def screenshot(name):
                # Capture only the real app, excluding the recording caption and pointer.
                page.evaluate("document.getElementById('cursor').style.visibility='hidden'")
                page.screenshot(path=str(SCREENSHOTS / name), clip={"x":0,"y":0,"width":1600,"height":828})
                page.evaluate("document.getElementById('cursor').style.visibility='visible'")

            def fill(label, value, delay=40):
                locator=app.get_by_label(label,exact=True)
                click(locator)
                locator.fill("")
                locator.press_sequentially(value,delay=delay)

            cue("01 / SSH PROFILE", "Add a deployment host with its address, SSH port, username, and authentication.")
            page.wait_for_timeout(1800)
            click(app.get_by_role("button",name="New host",exact=True).first)
            fill("Label","staging-web")
            app.get_by_label("Group", exact=True).select_option("staging-group")
            fill("Host name or IP","192.0.2.42")
            fill("Port","22")
            fill("Username","deploy")
            fill("Password","demo-only-password",45)
            page.wait_for_timeout(1400)
            cue("CONNECT", "Save & connect opens the terminal. Credentials are not sent until the server key is trusted.")
            click(app.get_by_role("button",name="Save & connect",exact=True))
            app.get_by_role("heading",name="Verify the server",exact=True).wait_for()
            page.screenshot(path=str(WORK / "verify-server.png"))
            cue("VERIFY FINGERPRINT", "Compare the Ed25519 SHA-256 fingerprint through a trusted channel before accepting.")
            page.wait_for_timeout(4800)
            click(app.get_by_role("button",name="Trust & connect",exact=True))
            app.get_by_role("button",name="Disconnect",exact=True).wait_for()
            page.wait_for_timeout(1000)
            cue("SSH TERMINAL", "Connected as deploy on staging-web. Confirm the hostname and current load.")
            click(app.locator(".xterm-screen"))
            page.keyboard.type("hostname; uptime",delay=90)
            page.keyboard.press("Enter")
            page.wait_for_timeout(2800)

            click(app.get_by_role("button", name="Edit staging-web", exact=True))
            app.get_by_role("dialog", name="Edit staging-web", exact=True).wait_for()
            app.locator(".drawer-body").evaluate("el => el.scrollTop = 0")
            page.wait_for_timeout(600)
            screenshot("ssh-profile.png")
            click(app.get_by_role("button", name="Cancel", exact=True))
            screenshot("workspace-dark.png")

            cue("02 / AI ASSISTANT", "Open AI Assistant. The demo provider is already configured and tool-calling verified.")
            click(app.get_by_role("button",name="AI Assistant",exact=True))
            app.get_by_label("Request",exact=True).wait_for(state="visible")
            page.wait_for_timeout(1800)
            fill("Session title","Staging nginx maintenance",45)
            app.get_by_label("Provider", exact=True).select_option("demo-provider")
            app.get_by_label("Model", exact=True).select_option("demo-tool-model")
            cue("SELECT TARGET", "Use @ to target staging-web explicitly; the selected host scopes this request.")
            fill("Request","@stag",150)
            page.wait_for_timeout(1300)
            click(app.get_by_label("Matching hosts",exact=True).get_by_role("button",name=re.compile("staging-web")))
            page.wait_for_timeout(1000)
            request=app.get_by_label("Request",exact=True)
            request.press_sequentially("Check nginx and disk space. Restart nginx if it is stopped.",delay=45)
            page.wait_for_timeout(1400)
            cue("RUN DIAGNOSTICS", "The assistant checks disk space and service state before proposing a privileged change.")
            click(app.get_by_role("button",name="Send",exact=True))
            approve=app.get_by_role("button",name="Approve as root",exact=True)
            approve.wait_for(state="visible")
            screenshot("ai-assistant.png")
            screenshot("approval.png")
            cue("REVIEW COMMAND", "Review the host and exact command: sudo systemctl restart nginx. Approve only this command.")
            page.wait_for_timeout(5200)
            click(approve)
            cue("VERIFY RESULT", "After approval, Selai runs the command and the assistant verifies nginx again.")
            app.get_by_role("heading",name="Verification",exact=True).wait_for()
            page.wait_for_timeout(2300)
            cue("COMPLETE", "nginx is active; restart exit code is 0. The response includes disk usage and the target host.")
            page.wait_for_timeout(4200)
            page.screenshot(path=str(OUTPUT / "selai-ssh-ai-demo-poster.jpg"),type="jpeg",quality=92)

            click(app.get_by_role("button", name="Close chat", exact=True))
            cue("03 / PORT FORWARDING", "Create an encrypted tunnel to reach a database through your SSH host.")
            click(app.get_by_role("navigation", name="Main", exact=True).get_by_role("button", name="Port forwarding", exact=True))
            click(app.get_by_role("button", name="New tunnel", exact=True))
            fill("Name", "Staging PostgreSQL")
            fill("Listen port", "15432")
            fill("Destination host", "db.internal")
            fill("Destination port", "5432")
            page.wait_for_timeout(1400)
            click(app.get_by_role("button", name="Save & start", exact=True))
            app.get_by_role("article", name="Staging PostgreSQL", exact=True).get_by_text("running", exact=True).wait_for()
            cue("MONITOR THE TUNNEL", "127.0.0.1:15432 reaches db.internal:5432. Inspect connections and traffic, or stop the tunnel.")
            page.wait_for_timeout(2400)
            # Show all three supported modes, using the same real editor for each.
            click(app.get_by_role("button", name="New tunnel", exact=True))
            fill("Name", "Preview webhook", 20)
            click(app.get_by_role("radio", name=re.compile("^Remote")))
            fill("Listen port", "9000", 20)
            fill("Destination port", "3000", 20)
            click(app.get_by_role("button", name="Save", exact=True))
            click(app.get_by_role("button", name="New tunnel", exact=True))
            fill("Name", "Staging SOCKS", 20)
            click(app.get_by_role("radio", name=re.compile("^Dynamic")))
            fill("Listen port", "1080", 20)
            click(app.get_by_role("button", name="Save", exact=True))
            cue("THREE TUNNEL MODES", "Save local, remote, or dynamic SOCKS5 tunnels. Start each route only when needed.")
            page.wait_for_timeout(1800)
            screenshot("port-forwarding.png")

            cue("04 / PROXIES", "Save reusable SOCKS5 or HTTP CONNECT routes for your SSH profiles.")
            click(app.get_by_role("navigation", name="Main", exact=True).get_by_role("button", name="Proxies", exact=True))
            click(app.get_by_role("button", name="New proxy", exact=True))
            fill("Name", "Office SOCKS5")
            fill("Proxy host", "socks.example.com")
            click(app.get_by_role("checkbox", name="Use proxy authentication", exact=True))
            fill("Username", "deploy")
            fill("Password", "demo-proxy-password", 20)
            page.wait_for_timeout(1200)
            click(app.get_by_role("button", name="Save proxy", exact=True))
            click(app.get_by_role("button", name="New proxy", exact=True))
            fill("Name", "Corporate HTTP", 20)
            app.get_by_label("Proxy protocol", exact=True).select_option("http")
            fill("Proxy host", "proxy.example.com", 20)
            click(app.get_by_role("button", name="Save proxy", exact=True))
            page.wait_for_timeout(1200)
            screenshot("proxies.png")

            cue("ASSIGN A ROUTE", "Enable a proxy in the SSH profile. Terminal, AI, SFTP, and forwarding share this route.")
            click(app.get_by_role("button", name="Edit staging-web", exact=True))
            click(app.get_by_role("checkbox", name="Connect through a proxy", exact=True))
            proxy = app.get_by_label("Proxy profile", exact=True)
            proxy.select_option(label="Office SOCKS5 / SOCKS5")
            page.wait_for_timeout(2300)
            screenshot("ssh-profile-proxy.png")
            click(app.get_by_role("button", name="Save", exact=True))
            cue("WORKSPACE READY", "One workspace for SSH profiles, AI assistance, port forwarding, and proxies.")
            page.wait_for_timeout(2500)
            duration=time.monotonic()-started
            frame=page.frames[1]
            calls=frame.evaluate("window.__DEMO_RECORDING__.calls")
            required = {"host_save", "host_trust", "terminal_connect", "agent_start", "approval_decide",
                        "provider_models", "forward_save", "forward_start", "proxy_save"}
            missing = required.difference(calls)
            if missing:
                raise RuntimeError(f"Demo is missing feature interactions: {sorted(missing)}")
            evidence = frame.evaluate("""() => ({
                hosts: window.__DEMO_RECORDING__.hosts.map(({label, groupId, proxyId}) => ({label,groupId,proxyId})),
                tunnels: window.__DEMO_RECORDING__.forwards.map(({profile, status}) => ({name:profile.name,mode:profile.mode,status})),
                proxies: window.__DEMO_RECORDING__.proxies.map(({name,kind}) => ({name,kind}))
            })""")
            assert len(evidence["tunnels"]) == 3
            assert len(evidence["proxies"]) == 2
            assert {v["kind"] for v in evidence["proxies"]} == {"socks5", "http"}
            assert any(v["status"] == "running" for v in evidence["tunnels"])
            assert evidence["hosts"][0]["proxyId"]
            assert {v["mode"] for v in evidence["tunnels"]} == {"local", "remote", "dynamic"}
            video=page.video
            raw=Path(video.path())
            context.close()
            browser.close()
        if errors:
            raise RuntimeError("Frontend errors during recording: "+"; ".join(errors))
        ffmpeg=imageio_ffmpeg.get_ffmpeg_exe()
        subprocess.run([ffmpeg,"-hide_banner","-loglevel","error","-y","-ss",f"{lead:.3f}","-i",str(raw),
            "-t",f"{duration:.3f}","-an","-c:v","libx264","-preset","medium","-crf","20",
            "-pix_fmt","yuv420p","-movflags","+faststart",str(OUTPUT / "selai-ssh-ai-demo.mp4")],check=True)
        srt=[]
        for i,(start,step,text) in enumerate(cues):
            end=cues[i+1][0] if i+1<len(cues) else duration
            srt.append(f"{i+1}\n{timestamp(start)} --> {timestamp(end)}\n{step}: {text}\n")
        (OUTPUT / "selai-ssh-ai-demo.en.srt").write_text("\n".join(srt),encoding="utf-8")
        (WORK / "recording-checks.json").write_text(json.dumps({"durationSeconds":duration,"frontendErrors":errors,
            "commands":sorted(set(calls)),"features":evidence,"mockBackend":True,"network":"loopback-only"},indent=2),encoding="utf-8")
        print(f"Recorded {duration:.1f}s, full-frame dark UI with English captions.",flush=True)
    finally:
        if os.name=="nt":
            subprocess.run(["taskkill","/PID",str(server.pid),"/T","/F"],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        else:
            server.terminate()
        log.close()


if __name__=="__main__":
    main()
