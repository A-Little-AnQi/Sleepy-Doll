"""通用快捷任务验收：真实 AppController，BGI 停用，仅使用本地模拟模型，不调用真实用户工具。"""
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ROOT = Path(__file__).resolve().parents[2]
TEMP = Path(tempfile.gettempdir()) / "sleepy-doll-task-workflow-validation"
process = None
server = None
server_thread = None
model_requests = []
model_binding = None
class FixtureModel(BaseHTTPRequestHandler):
    def log_message(self, *_args): pass
    def do_POST(self):
        request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        model_requests.append(request)
        if len(model_requests) == 1:
            message = {"role": "assistant", "content": "", "tool_calls": [{"id": "save-entry", "type": "function", "function": {"name": "shortcut.save", "arguments": json.dumps({"id": "wrong-task-id", "name": "Configured chosen item", "binding": model_binding})}}]}
        else:
            message = {"role": "assistant", "content": "已加入快捷任务，可以点击运行。"}
        body = json.dumps({"choices": [{"message": message, "finish_reason": "tool_calls" if len(model_requests) == 1 else "stop"}], "usage": {"prompt_tokens": 100, "completion_tokens": 20}}).encode()
        self.send_response(200); self.send_header("Content-Type", "application/json"); self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)
if TEMP.exists():
    raise RuntimeError("夹具目录已存在，不覆盖未知数据")
TEMP.mkdir()
try:
    config = json.loads((ROOT / "dist/Sleepy-Doll/user/config.json").read_text(encoding="utf-8-sig"))
    server = ThreadingHTTPServer(("127.0.0.1", 0), FixtureModel)
    server_thread = threading.Thread(target=server.serve_forever, daemon=True); server_thread.start()
    print(json.dumps({"modelFixturePort": server.server_port, "cleanup": "finally closes server and isolated driver"}), flush=True)
    config["version"] = 5
    config["hooks"] = []
    for model in config["models"]:
        model.update(apiKey="unused-fixture-key", baseUrl="http://127.0.0.1:9", headers={})
    config["models"] = [{"id": "fixture", "name": "Fixture", "protocol": "openai-chat", "model": "fixture", "baseUrl": f"http://127.0.0.1:{server.server_port}", "apiKey": "unused-fixture-key"}]
    config["activeModel"] = "fixture"
    config["agent"].update(skillDirectories=[], fallbackModels=[])
    config["plugins"] = {"directories": [], "enabled": [], "disabled": ["bgi"]}
    config["bridge"]["enabled"] = False
    config["storage"] = {"database": str(TEMP / "data.db")}
    config["runtime"].update(catalogDirectory=str(TEMP), permissionMode="fullAccess", grants=[], trustGrants=[])
    path = TEMP / "config.json"
    path.write_text(json.dumps(config), encoding="utf-8")
    process = subprocess.Popen([str(ROOT / "target/debug/bgi-agent-check.exe"), str(path)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8")

    def rpc(method, params):
        process.stdin.write(json.dumps({"method": method, "params": params}) + "\n")
        process.stdin.flush()
        raw = process.stdout.readline()
        if not raw:
            raise RuntimeError("驱动器退出：" + process.stderr.read()[:1000])
        value = json.loads(raw)
        assert value["ok"], value
        return value["result"]

    def save(name, nodes):
        draft = rpc("workflow.draft.create", {"name": name, "description": "通用确定性流程", "nodes": nodes})
        assert not draft["validation"]["issues"] and not draft["validation"]["missingBindings"], draft
        rpc("workflow.publish", {"id": draft["id"], "draftRevision": draft["revision"]})
        return draft["id"]

    def run(task, expected="succeeded"):
        value = rpc("workflow.run", {"id": task})
        end = time.monotonic() + 15
        while value["state"] in {"queued", "preflighting", "running", "executing", "verifying", "waitingJob", "deciding"} and time.monotonic() < end:
            time.sleep(0.1)
            value = rpc("run.get", {"id": value["id"]})
        assert value["state"] == expected, value
        assert value.get("inputTokens", 0) == 0 and value.get("outputTokens", 0) == 0, value
        run.last = value
        return value["result"]

    (TEMP / "input.txt").write_text("first.txt", encoding="utf-8")
    (TEMP / "first.txt").write_text("FIRST", encoding="utf-8")
    (TEMP / "second.txt").write_text("SECOND", encoding="utf-8")
    task = save("读取本次数据", [
        {"kind": "tool", "id": "path", "title": "读取目标路径", "tool": "workspace.read", "arguments": {"path": "input.txt"}},
        {"kind": "tool", "id": "content", "title": "读取本次目标", "tool": "workspace.read", "arguments": {"path": {"$ref": {"kind": "nodeOutput", "node": "path", "path": ["text"]}}}},
        {"kind": "result", "id": "result", "title": "整理输出", "template": "{{ nodes.content.output.text }}"},
    ])
    first = run(task)
    (TEMP / "input.txt").write_text("second.txt", encoding="utf-8")
    second = run(task)
    assert "FIRST" in first and "SECOND" in second and "FIRST" not in second, (first, second)

    repeated = save("按本轮结果结束循环", [{"kind": "repeat", "id": "loop", "title": "最多三轮", "maxIterations": 3,
        "nodes": [{"kind": "result", "id": "body", "title": "本轮序号", "template": "{{ iteration }}"}],
        "until": {"kind": "compare", "op": "equals", "left": {"kind": "nodeOutput", "node": "body", "path": ["text"]}, "right": {"kind": "literal", "value": "2"}}},
        {"kind": "result", "id": "result", "title": "循环后的本次结果", "template": "{{ nodes.body.output.text }}"}])
    repeated_result = run(repeated)
    assert "2" in repeated_result, repeated_result
    events = rpc("events.read", {"conversationId": run.last["conversationId"], "after": 0})["events"]
    assert any(event["kind"] == "step.finished" and event["data"].get("id") == "loop" and event["data"].get("detail") == "已执行 2 次" for event in events), events

    (TEMP / "ready.txt").write_text("WAIT", encoding="utf-8")
    waiting = save("只读检查满足后继续", [{"kind": "wait", "id": "wait", "title": "等待本次状态", "timeoutSeconds": 8, "checkSeconds": 1,
        "probe": {"id": "probe", "title": "读取最新状态", "tool": "workspace.read", "arguments": {"path": "ready.txt"}},
        "until": {"kind": "compare", "op": "equals", "left": {"kind": "nodeOutput", "node": "probe", "path": ["text"]}, "right": {"kind": "literal", "value": "READY"}}},
        {"kind": "result", "id": "result", "title": "检查结果", "template": "{{ nodes.probe.output.text }}"}])
    update = threading.Thread(target=lambda: (time.sleep(0.5), (TEMP / "ready.txt").write_text("READY", encoding="utf-8")))
    update.start()
    waited = run(waiting)
    update.join()
    assert "READY" in waited, waited
    assert all(item["zeroToken"] and item["runnable"] for item in rpc("workflow.list", {}))
    binding = {"applicationName": "Fixture application", "targetName": "Chosen file", "prepare": [{"tool": "workspace.read", "arguments": {"path": "input.txt"}}], "action": {"tool": "workspace.read", "arguments": {"path": "first.txt"}}}
    shortcut = rpc("shortcut.save", {"name": "Only the chosen item", "binding": binding})
    shortcut_id = shortcut["taskId"]
    summary = next(item for item in rpc("workflow.list", {}) if item["id"] == shortcut_id)
    assert summary["shortcut"] == binding and summary["runnable"] and summary["nodeCount"] == 2, summary
    assert not rpc("task.list", {}) or not any(item.get("source", {}).get("workflowId") == shortcut_id for item in rpc("task.list", {})), "Saving must not execute"
    run(shortcut_id)
    assert run.last["toolCalls"] == 2, run.last
    (TEMP / "first.txt").write_text("CHANGED", encoding="utf-8")
    run(shortcut_id)
    assert run.last["toolCalls"] == 2, run.last
    assert all(item["id"] != run.last["conversationId"] for item in rpc("conversation.list", {})), "Task execution must not appear as a chat"
    (TEMP / "first.txt").unlink()
    run(shortcut_id, "failed")
    assert run.last["toolCalls"] == 2 and run.last["error"], run.last
    binding["action"]["arguments"]["path"] = "second.txt"
    updated = rpc("shortcut.save", {"id": shortcut_id, "name": "Updated chosen item", "binding": binding})
    assert updated["taskId"] == shortcut_id and updated["publishedRevision"] == 2, updated
    run(shortcut_id)
    assert run.last["toolCalls"] == 2, run.last
    assert sum(item["id"] == shortcut_id for item in rpc("workflow.list", {})) == 1
    assert not model_requests, "Direct executions must never call the model"
    model_binding = binding
    configured = rpc("shortcut.configure", {"prompt": "修改这个快捷任务，只保存，不运行", "shortcutId": shortcut_id, "clientKey": "configuration-fixture"})
    end = time.monotonic() + 15
    while configured["state"] not in {"answered", "succeeded", "failed", "cancelled", "needsReview", "awaitingApproval"} and time.monotonic() < end:
        time.sleep(0.1)
        configured = rpc("run.get", {"id": configured["id"]})
    assert configured["state"] == "answered", configured
    assert len(model_requests) == 2 and any(tool["function"]["name"].startswith("shortcut_save_") for tool in model_requests[0]["tools"]), {"requests": len(model_requests), "tools": [tool["function"]["name"] for tool in model_requests[0]["tools"]], "run":configured}
    assert any(shortcut_id in message.get("content", "") and "当前绑定" in message.get("content", "") for message in model_requests[0]["messages"] if message["role"] == "system")
    summary = next(item for item in rpc("workflow.list", {}) if item["id"] == shortcut_id)
    assert summary["name"] == "Configured chosen item" and summary["publishedRevision"] == 3
    assert not any(item["id"] == "wrong-task-id" for item in rpc("workflow.list", {})), "Editing must update only the selected task"
    assert configured["conversationId"].startswith("shortcut-config-")
    assert all(item["id"] != configured["conversationId"] for item in rpc("conversation.list", {}))
    saved_events = rpc("events.read", {"conversationId": configured["conversationId"], "after": 0})["events"]
    assert any(event["kind"] == "shortcut.saved" and event["data"]["taskId"] == shortcut_id for event in saved_events)
    run(shortcut_id); run(shortcut_id)
    assert len(model_requests) == 2, "Launching saved task must not continue AI configuration"
    print(json.dumps({"aiConfigurationCalls": 2, "selectedEntryUpdatedDespiteWrongModelId": True, "configurationDoesNotRun": True, "savedEventDelivered": True, "subsequentRunsModelCalls": 0}), flush=True)
    print(json.dumps({"shortcutSaveDoesNotRun": True, "oneChosenAction": True, "missingTargetDetectedWithoutModelRetry": True, "updatePreservesEntry": True, "taskRunsHiddenFromChatList": True, "realModelRequests": 0}), flush=True)
    print(json.dumps({"latestInputEachRun": True, "repeatUntilUsesCurrentOutput": True, "waitPollsCurrentProbe": True, "bgiDisabled": True, "realModelRequests": 0, "realUserDataTouched": False}), flush=True)
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
        for stream in [process.stdin, process.stdout, process.stderr]:
            try:
                stream.close()
            except OSError:
                pass
    if server is not None:
        server.shutdown(); server.server_close()
    if server_thread is not None:
        server_thread.join(timeout=10)
    if TEMP.resolve().parent == Path(tempfile.gettempdir()).resolve():
        shutil.rmtree(TEMP)
