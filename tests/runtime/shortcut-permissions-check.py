"""真实 AppController 验证已保存任务直接执行；不连接模型、BGI 或用户数据库。"""
import json
import os
from pathlib import Path
import queue
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ROOT = Path(__file__).resolve().parents[2]
RUNTIME = ROOT / "target/.tmp/shortcut-permissions"
FIXTURE = RUNTIME / "driver"
process = None
reader = None
model_server = None
model_thread = None
model_requests = []
fixture_owned = False


class ModelTrap(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def do_POST(self):
        model_requests.append(self.path)
        self.send_error(503, "Saved tasks must not call models")


def main():
    global process, reader, model_server, model_thread, fixture_owned
    if FIXTURE.exists():
        raise RuntimeError(f"已有验证数据，不覆盖：{FIXTURE}")
    FIXTURE.mkdir(parents=True)
    fixture_owned = True
    model_server = ThreadingHTTPServer(("127.0.0.1", 0), ModelTrap)
    model_thread = threading.Thread(target=model_server.serve_forever)
    model_thread.start()
    config = json.loads((ROOT / "sleepy-doll.config.example.json").read_text(encoding="utf-8"))
    config.update(activeModel="unused", hooks=[])
    config["models"] = [{"id": "unused", "name": "Unused", "protocol": "openai-chat",
                         "model": "unused", "baseUrl": f"http://127.0.0.1:{model_server.server_port}", "apiKey": "fixture"}]
    config["bridge"].update(enabled=False, token=None)
    config["agent"].update(skillDirectories=[], fallbackModels=[])
    config["plugins"] = {"directories": [], "enabled": [], "disabled": ["bgi"]}
    config["storage"] = {"database": str(FIXTURE / "data.db")}
    config["runtime"].update(catalogDirectory=str(FIXTURE), permissionMode="askEach", grants=[], trustGrants=[])
    config_path = FIXTURE / "config.json"
    config_path.write_text(json.dumps(config), encoding="utf-8")
    env = dict(os.environ, TEMP=str(RUNTIME), TMP=str(RUNTIME), TMPDIR=str(RUNTIME))
    process = subprocess.Popen([str(ROOT / "target/debug/bgi-agent-check.exe"), str(config_path)],
                               stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                               text=True, encoding="utf-8", env=env)
    responses = queue.Queue()

    def receive():
        for line in process.stdout:
            responses.put(json.loads(line))
        responses.put(None)

    reader = threading.Thread(target=receive)
    reader.start()
    print(json.dumps({"driverPid": process.pid, "modelTrapPort": model_server.server_port, "fixture": str(FIXTURE)}), flush=True)

    def rpc(method, params, ok=True):
        process.stdin.write(json.dumps({"method": method, "params": params}) + "\n")
        process.stdin.flush()
        deadline = time.monotonic() + 10
        while True:
            response = responses.get(timeout=max(0.01, deadline - time.monotonic()))
            if response is None:
                raise RuntimeError("验证驱动已退出")
            if "ok" in response:
                assert response["ok"] == ok, response
                return response.get("result") if ok else response

    for mode in ["askEach", "planOnly", "trustedScopes", "fullAccess"]:
        rpc("permission.set", {"mode": mode})
        marker = f"{mode}.txt"
        task = rpc("shortcut.save", {"name": f"Direct {mode}", "binding": {
            "applicationName": "Fixture", "targetName": marker,
            "action": {"tool": "workspace.write", "arguments": {"path": marker, "content": mode}}
        }})
        assert not (FIXTURE / marker).exists(), "封装不能提前执行"
        rpc("workflow.run", {"id": task["taskId"], "expectedPublishedRevision": 999}, ok=False)
        assert not (FIXTURE / marker).exists(), "过期任务修订不能执行"
        run = rpc("workflow.run", {"id": task["taskId"], "expectedPublishedRevision": task["publishedRevision"]})
        deadline = time.monotonic() + 8
        while run["state"] in ["queued", "preflighting", "running", "executing", "verifying", "waitingJob", "deciding"]:
            assert time.monotonic() < deadline, run
            time.sleep(0.05)
            run = rpc("run.get", {"id": run["id"]})
        assert run["state"] == "succeeded", run
        assert (FIXTURE / marker).read_text(encoding="utf-8") == mode
        assert run.get("inputTokens", 0) == 0 and run.get("outputTokens", 0) == 0, run
        assert rpc("run.usage", {"id": run["id"]})["byPurpose"] == [], "固定入口不应产生模型记账"
        events = rpc("events.read", {"conversationId": run["conversationId"], "after": 0})["events"]
        assert not any(event["kind"] == "approval.requested" for event in events), events
        assert rpc("bootstrap", {})["permission"]["mode"] == mode, "不能修改对话审批设置"
    assert not model_requests, model_requests
    identity = rpc("app.identity", {})
    assert len(identity["revision"]) == 40 and identity["schemaVersion"] >= 18, identity
    print(json.dumps({"savedTaskModes": 4, "approvalRequests": 0, "modelRequests": 0,
                      "revisionMismatchRejected": True, "dialoguePermissionPreserved": True}), flush=True)


try:
    main()
finally:
    if process is not None:
        if process.poll() is None:
            try:
                process.stdin.write('{"method":"shutdown"}\n')
                process.stdin.flush()
                process.wait(timeout=10)
            except (OSError, subprocess.TimeoutExpired):
                process.kill()
                process.wait(timeout=10)
        process.stdin.close()
        process.stdout.close()
    if reader is not None:
        reader.join(timeout=3)
    if model_server is not None:
        model_server.shutdown()
        model_server.server_close()
    if model_thread is not None:
        model_thread.join(timeout=3)
    if fixture_owned:
        cleanup = subprocess.run(["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                                  r"E:\tools\Remove-Directory.ps1", "-Path", str(FIXTURE), "-Json"],
                                 env=dict(os.environ, TEMP=str(RUNTIME), TMP=str(RUNTIME), TMPDIR=str(RUNTIME)),
                                 capture_output=True, text=True)
        if cleanup.returncode != 0 or FIXTURE.exists():
            raise RuntimeError(f"验证数据清理失败：{FIXTURE}\n{cleanup.stdout}\n{cleanup.stderr}")
