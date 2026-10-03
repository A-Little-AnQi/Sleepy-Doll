"""通用快捷任务验收：真实 AppController，BGI 停用，仅使用本地模拟模型，不调用真实用户工具。"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ROOT = Path(__file__).resolve().parents[2]
# 本任务已登记的任务根目录；所有子进程 TEMP/TMP 都重定向到这里。
TASK_TEMP = ROOT / "target/.tmp/shortcut-runtime"
TEMP = TASK_TEMP / "workflow-fixture"
process = None
server = None
server_thread = None
model_requests = []
model_binding = None
reference_mode = False
class FixtureModel(BaseHTTPRequestHandler):
    def log_message(self, *_args): pass
    def do_POST(self):
        request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        model_requests.append(request)
        if reference_mode:
            message = {"role": "assistant", "content": "REF_EXISTING_ITEM_ONLY: second.txt is already available."}
        elif len(model_requests) == 1:
            message = {"role": "assistant", "content": "", "tool_calls": [{"id": "forbidden-write", "type": "function", "function": {"name": "workspace.write", "arguments": json.dumps({"path": "unexpected.txt", "content": "MUST_NOT_BE_CREATED"})}}]}
        elif len(model_requests) == 2:
            message = {"role": "assistant", "content": "", "tool_calls": [{"id": "preview-entry", "type": "function", "function": {"name": "shortcut.save", "arguments": json.dumps({"id": "wrong-task-id", "name": "Configured chosen item", "binding": model_binding})}}]}
        else:
            message = {"role": "assistant", "content": "Unexpected additional model request"}
        body = json.dumps({"choices": [{"message": message, "finish_reason": "tool_calls" if message.get("tool_calls") else "stop"}], "usage": {"prompt_tokens": 100, "completion_tokens": 20}}).encode()
        self.send_response(200); self.send_header("Content-Type", "application/json"); self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)
TASK_TEMP.mkdir(parents=True, exist_ok=True)
if TEMP.exists():
    raise RuntimeError("夹具目录已存在，不覆盖未知数据")
TEMP.mkdir()
stderr_tail = []
try:
    config = json.loads((ROOT / "dist/Sleepy-Doll/user/config.json").read_text(encoding="utf-8-sig"))
    server = ThreadingHTTPServer(("127.0.0.1", 0), FixtureModel)
    server_thread = threading.Thread(target=server.serve_forever, daemon=True); server_thread.start()
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
    env = dict(os.environ)
    env["TEMP"] = str(TASK_TEMP)
    env["TMP"] = str(TASK_TEMP)
    process = subprocess.Popen([str(ROOT / "target/debug/bgi-agent-check.exe"), str(path)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8", env=env)
    # 后台持续排空 stderr，避免管道写满死锁；尾部留作失败诊断。
    threading.Thread(target=lambda: [stderr_tail.append(line) for line in process.stderr], daemon=True).start()
    print(json.dumps({"modelFixturePort": server.server_port, "driverPid": process.pid, "taskTemp": str(TASK_TEMP), "cleanup": "finally closes server and isolated driver, removes verified fixture dir"}), flush=True)

    def rpc(method, params):
        process.stdin.write(json.dumps({"method": method, "params": params}) + "\n")
        process.stdin.flush()
        raw = process.stdout.readline()
        if not raw:
            raise RuntimeError("驱动器退出：" + "".join(stderr_tail)[-1500:])
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
    skills = rpc("bootstrap", {})["skills"]
    embedded = next((item for item in skills if item["name"] == "create-shortcut"), None)
    assert embedded and embedded["source"] == "product" and embedded["available"], skills
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
    # 三步快捷任务：第二步读缺失文件失败后，第三步不得执行（真实驱动停链验证）。
    chain_binding = {"applicationName": "Fixture application", "targetName": "Three-step chain", "steps": [
        {"title": "写入第一个标记", "action": {"tool": "workspace.write", "arguments": {"path": "shortcut-marker-1.txt", "content": "ONE"}}},
        {"title": "读取缺失文件", "action": {"tool": "workspace.read", "arguments": {"path": "shortcut-missing.txt"}}},
        {"title": "写入第三个标记", "action": {"tool": "workspace.write", "arguments": {"path": "shortcut-marker-3.txt", "content": "THREE"}}},
    ]}
    chain = rpc("shortcut.save", {"name": "Three-step chain", "binding": chain_binding})
    chain_id = chain["taskId"]
    assert not (TEMP / "shortcut-marker-1.txt").exists() and not (TEMP / "shortcut-marker-3.txt").exists(), "Saving steps must not execute"
    run(chain_id, "failed")
    assert run.last["error"], run.last
    assert (TEMP / "shortcut-marker-1.txt").exists() and not (TEMP / "shortcut-marker-3.txt").exists(), run.last
    chain_events = rpc("events.read", {"conversationId": run.last["conversationId"], "after": 0})["events"]
    assert any(event["kind"] == "step.finished" and event["data"].get("id") == "step-1" and event["data"].get("outcome") == "verifiedFailed" for event in chain_events), chain_events
    assert any(event["kind"] == "step.finished" and event["data"].get("id") == "step-2" and event["data"].get("outcome") == "skipped" for event in chain_events), chain_events
    assert not any(event["kind"] == "tool.started" and "step-2" in json.dumps(event["data"]) for event in chain_events), "第三步不得启动任何工具调用"
    # 全部有效的组合：更新同入口（不建副本），连续两次运行都零 token、零模型调用。
    chain_binding["steps"][1]["action"]["arguments"]["path"] = "input.txt"
    updated_chain = rpc("shortcut.save", {"id": chain_id, "name": "Three-step chain", "binding": chain_binding})
    assert updated_chain["taskId"] == chain_id, updated_chain
    # workspace.write 对已存在目标要求 expectedSha256；每轮运行前清场保证可重复。
    (TEMP / "shortcut-marker-1.txt").unlink()
    (TEMP / "shortcut-marker-3.txt").unlink(missing_ok=True)
    run(chain_id)
    assert (TEMP / "shortcut-marker-1.txt").exists() and (TEMP / "shortcut-marker-3.txt").exists(), run.last
    (TEMP / "shortcut-marker-1.txt").unlink()
    (TEMP / "shortcut-marker-3.txt").unlink()
    run(chain_id)
    assert run.last["toolCalls"] == 3, run.last
    assert (TEMP / "shortcut-marker-1.txt").exists() and (TEMP / "shortcut-marker-3.txt").exists(), run.last
    assert sum(item["id"] == chain_id for item in rpc("workflow.list", {})) == 1, "Updating the chain must not create a duplicate"
    assert not model_requests, "Direct executions must never call the model"
    reference_mode = True
    source_run = rpc("task.submit", {"prompt": "只确认已有项目，不修改", "clientKey": "reference-fixture"})
    end = time.monotonic()+10
    while source_run["state"] != "answered" and time.monotonic()<end:
        time.sleep(0.1); source_run = rpc("run.get", {"id":source_run["id"]})
    assert source_run["state"] == "answered", source_run
    reference_mode = False
    model_requests.clear()
    model_binding = binding
    configured = rpc("shortcut.configure", {"prompt": "修改这个快捷任务，只保存，不运行", "shortcutId": shortcut_id, "clientKey": "configuration-fixture", "referenceConversationId":source_run["conversationId"]})
    end = time.monotonic() + 15
    while configured["state"] not in {"answered", "succeeded", "failed", "cancelled", "needsReview", "awaitingApproval"} and time.monotonic() < end:
        time.sleep(0.1)
        configured = rpc("run.get", {"id": configured["id"]})
    assert configured["state"] == "answered", configured
    assert len(model_requests) == 2 and any(tool["function"]["name"].startswith("shortcut_save_") for tool in model_requests[0]["tools"]), {"requests": len(model_requests), "tools": [tool["function"]["name"] for tool in model_requests[0]["tools"]], "run":configured}
    assert any(shortcut_id in message.get("content", "") and "当前绑定" in message.get("content", "") for message in model_requests[0]["messages"] if message["role"] == "system")
    assert not (TEMP / "unexpected.txt").exists(), "Wrapping must reject writing or creating existing resources"
    assert any("REF_EXISTING_ITEM_ONLY" in message.get("content", "") for message in model_requests[0]["messages"] if message["role"] == "system")
    summary = next(item for item in rpc("workflow.list", {}) if item["id"] == shortcut_id)
    assert summary["name"] == "Updated chosen item" and summary["publishedRevision"] == 2, "Preview must not save automatically"
    proposal_events = rpc("events.read", {"conversationId":configured["conversationId"],"after":0})["events"]
    assert any(event["kind"] == "shortcut.proposed" for event in proposal_events)
    assert not any(event["kind"] == "shortcut.saved" for event in proposal_events)
    accepted = rpc("shortcut.accept", {"runId":configured["id"],"name":"Configured chosen item","description":"Wrap the existing file"})
    assert accepted["taskId"] == shortcut_id
    assert rpc("shortcut.accept", {"runId":configured["id"],"name":"Configured chosen item","description":"Wrap the existing file"})["taskId"] == shortcut_id, "Retry must not create a duplicate"
    summary = next(item for item in rpc("workflow.list", {}) if item["id"] == shortcut_id)
    assert summary["sourceConversationId"] == source_run["conversationId"] or summary["sourceConversationId"] is None
    assert summary["name"] == "Configured chosen item" and summary["publishedRevision"] == 3
    assert not any(item["id"] == "wrong-task-id" for item in rpc("workflow.list", {})), "Editing must update only the selected task"
    assert configured["conversationId"].startswith("shortcut-config-")
    assert all(item["id"] != configured["conversationId"] for item in rpc("conversation.list", {}))
    saved_events = rpc("events.read", {"conversationId": configured["conversationId"], "after": 0})["events"]
    assert any(event["kind"] == "shortcut.saved" and event["data"]["taskId"] == shortcut_id for event in saved_events)
    run(shortcut_id); run(shortcut_id)
    assert len(model_requests) == 2, "Launching saved task must not continue AI configuration"
    print(json.dumps({"aiConfigurationCalls": 2, "selectedEntryUpdatedDespiteWrongModelId": True, "wrapExistingOnly": True, "referenceContextSupplied":True, "writesDuringWrappingRejected":True, "previewRequiresSave":True, "acceptRetryIsIdempotent":True, "savedEventDelivered": True, "subsequentRunsModelCalls": 0}), flush=True)
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
    resolved = TEMP.resolve()
    if resolved.parent == TASK_TEMP.resolve() and resolved != TASK_TEMP.resolve():
        shutil.rmtree(resolved)
    else:
        print(json.dumps({"cleanupSkipped": str(resolved), "reason": "夹具目录不在登记的任务根目录下，需人工核对"}), flush=True)
