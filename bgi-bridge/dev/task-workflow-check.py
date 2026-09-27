"""通用快捷任务验收：真实 AppController，BGI 停用，不调用模型或真实用户工具。"""
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parents[2]
TEMP = Path(tempfile.gettempdir()) / "sleepy-doll-task-workflow-validation"
process = None
if TEMP.exists():
    raise RuntimeError("夹具目录已存在，不覆盖未知数据")
TEMP.mkdir()
try:
    config = json.loads((ROOT / "dist/Sleepy-Doll/user/config.json").read_text(encoding="utf-8-sig"))
    config["version"] = 5
    config["hooks"] = []
    for model in config["models"]:
        model.update(apiKey="unused-fixture-key", baseUrl="http://127.0.0.1:9", headers={})
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

    def run(task):
        value = rpc("workflow.run", {"id": task})
        end = time.monotonic() + 15
        while value["state"] in {"queued", "preflighting", "running", "executing", "verifying", "waitingJob", "deciding"} and time.monotonic() < end:
            time.sleep(0.1)
            value = rpc("run.get", {"id": value["id"]})
        assert value["state"] == "succeeded", value
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
    print(json.dumps({"latestInputEachRun": True, "repeatUntilUsesCurrentOutput": True, "waitPollsCurrentProbe": True, "bgiDisabled": True, "modelRequests": 0, "realUserDataTouched": False}), flush=True)
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
    if TEMP.resolve().parent == Path(tempfile.gettempdir()).resolve():
        shutil.rmtree(TEMP)
