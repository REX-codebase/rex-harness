#!/usr/bin/env python3
"""Scripted OpenAI-compatible server for verification/usage-flow/e2e-flow.py. Models a flaky free host:
req 1 -> Groq-shaped 429 (try again in 2s); req 2 -> a 22s response (the old 20s transport limit
would have killed it); then build a file, stall by re-reading, and only complete when the harness
runs its final check (tools narrowed to <=3). Logs metadata only."""
import json, sys, time, threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
LOG=sys.argv[1]; lock=threading.Lock(); state={"n":0,"created":False,"stage":0}
def log(m):
    with lock: open(LOG,"a").write(m+"\n")
def call(i,name,args): return {"id":f"call_{i}","type":"function","function":{"name":name,"arguments":json.dumps(args)}}
class H(BaseHTTPRequestHandler):
    protocol_version="HTTP/1.1"
    def log_message(self,*a): pass
    def _send(self,code,obj):
        data=json.dumps(obj).encode(); self.send_response(code); self.send_header("Content-Type","application/json"); self.send_header("Content-Length",str(len(data))); self.end_headers(); self.wfile.write(data)
    def do_GET(self):
        if self.path.endswith("/models"): return self._send(200,{"object":"list","data":[{"id":"mock-1"}]})
        self._send(404,{})
    def do_POST(self):
        n=int(self.headers.get("Content-Length") or 0); body=json.loads(self.rfile.read(n) or b"{}")
        auth=self.headers.get("Authorization","")
        with lock: state["n"]+=1; k=state["n"]
        tools=[t["function"]["name"] for t in body.get("tools",[])]
        log(f"req#{k} auth_ok={auth=='Bearer sk-mock-key'} tools={len(tools)} msgs={len(body.get('messages',[]))}")
        if k==1: return self._send(429,{"error":{"message":"Rate limit reached for model `mock-1` on tokens per minute (TPM): Limit 8000, Used 6062, Requested 6439. Please try again in 2s. Need more tokens?","type":"tokens","code":"rate_limit_exceeded"}})
        if k==2: time.sleep(22)
        if len(tools)<=3 and "complete_task" in tools:
            msg={"role":"assistant","content":None,"tool_calls":[call(k,"update_plan",{"items":[{"id":"1","title":"make hello.txt","status":"done"}]}),call(k+1000,"complete_task",{"summary":"hello.txt was created and read back"})]}
        elif not state["created"]:
            state["created"]=True
            msg={"role":"assistant","content":None,"tool_calls":[call(k,"update_plan",{"items":[{"id":"1","title":"make hello.txt","status":"in_progress"}]}),call(k+1000,"create_file",{"path":"hello.txt","content":"hello from the mock","overwrite":True})]}
        else:
            msg={"role":"assistant","content":None,"tool_calls":[call(k,"read_file",{"path":"hello.txt"})]}
        self._send(200,{"choices":[{"index":0,"message":msg,"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":6000,"completion_tokens":50,"total_tokens":6050}})
ThreadingHTTPServer(("127.0.0.1",18080),H).serve_forever()
