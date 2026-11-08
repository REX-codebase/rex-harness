#!/usr/bin/env python3
"""End-to-end check of the usage flow against the REAL backend binary.

    cargo build -p rex-mcp --bin rex-dev-server
    python3 verification/usage-flow/e2e-flow.py

A scripted OpenAI-compatible server (mock-openai.py) plays a flaky free host:
a Groq-shaped 429, then a 22 second response, then a model that builds a file
and stalls re-reading it. Everything is configured through the sidecar's HTTP
API with NO relay and NO --store-file, so it fails if the key or custom base
URL do not reach the agent. Exits 1 on any failed check.
"""
import json, os, signal, subprocess, sys, tempfile, time, urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
SERVER = os.environ.get("REX_DEV_SERVER", os.path.join(ROOT, "target", "debug", "rex-dev-server"))
tmp = tempfile.mkdtemp(prefix="rex-usage-flow-")
mock_log = os.path.join(tmp, "mock.log")
open(mock_log, "w").close()
results = []

def check(name, ok, detail=""):
    results.append(ok)
    print(("PASS  " if ok else "FAIL  ") + name + (f"  {detail}" if detail else ""))

def api(method, path, body=None):
    req = urllib.request.Request("http://127.0.0.1:8787" + path,
                                 data=(json.dumps(body).encode() if body is not None else None),
                                 method=method, headers={"Content-Type": "application/json"})
    return json.loads(urllib.request.urlopen(req, timeout=60).read())

if not os.path.exists(SERVER):
    sys.exit(f"build the backend first: {SERVER} not found")
mock = subprocess.Popen([sys.executable, os.path.join(HERE, "mock-openai.py"), mock_log])
srv = subprocess.Popen([SERVER], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
try:
    for _ in range(80):
        try:
            api("GET", "/api/status"); break
        except Exception:
            time.sleep(0.25)
    check("key and custom base URL accepted through the API",
          api("POST", "/api/providers/custom/key", {"key": "sk-mock-key", "base_url": "http://127.0.0.1:18080/v1"}).get("ok") is True)
    view = api("POST", "/api/agent/custody/runs",
               {"task": "Create hello.txt then confirm it", "provider": "custom", "model": "mock-1", "plan_mode": False})
    check("the agent run starts on the custom endpoint (no 'no base URL configured')", "error" not in view, view.get("error", ""))
    rid = view["snapshot"]["id"]
    t0 = time.time(); retries = []; infos = []; gates = []
    seen = set(); snap = {}
    while time.time() - t0 < 150:
        snap = api("GET", f"/api/agent/runs/{rid}")
        for e in snap["events"]:
            k = json.dumps(e, sort_keys=True)
            if k in seen:
                continue
            seen.add(k)
            if e.get("state") == "retry": retries.append(e["reason"])
            elif e.get("state") == "info": infos.append(e["message"])
            elif e.get("state") == "gate_result": gates.append(e["passed"])
        if snap.get("pending_approval"):
            api("POST", f"/api/agent/runs/{rid}/decision", {"approved": True})
        if snap.get("terminal_reason"):
            break
        time.sleep(0.4)
    elapsed = time.time() - t0
    log = open(mock_log).read()
    check("every request reached the mock with the saved key", log.count("auth_ok=True") >= 5 and "auth_ok=False" not in log)
    check("a 429 produced a readable wait, not a raw error",
          any(r.startswith("rate limited by the provider; waiting") for r in retries), str(retries[-1:]))
    check("the 22 s response succeeded (old transport limit was 20 s)", elapsed > 22 and "tools=17 msgs=4" in log, f"{elapsed:.1f}s")
    check("the stall triggered the final check", any("offering one final check" in m for m in infos))
    check("the final-check request offered only 3 tools", "tools=3" in log)
    check("the completion claim went through the gates and passed", gates == [True], str(gates))
    check("the run ended completed", (snap.get("terminal_reason") or {}).get("kind") == "completed", json.dumps(snap.get("terminal_reason")))
finally:
    for p in (srv, mock):
        p.send_signal(signal.SIGTERM)
failed = results.count(False)
print(f"\n{len(results) - failed}/{len(results)} checks passed")
sys.exit(1 if failed else 0)
