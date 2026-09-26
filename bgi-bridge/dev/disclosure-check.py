"""实际 Supervisor + 已配置模型的隔离行为检查；不连接真实 BGI。

运行前编译 cargo build --no-default-features --features dev-ipc --bin bgi-agent-check。
模型密钥只保存在进程内存，临时配置仅使用代理占位密钥。测试数据库自动清理。
"""
import argparse
import copy
import hashlib
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ROOT = Path(__file__).resolve().parents[2]
TEMP = Path(tempfile.gettempdir()) / "sleepy-doll-bgi-disclosure-validation"
TERMINAL = {"answered", "succeeded", "partial", "failed", "cancelled", "needsReview", "blocked", "awaitingUser", "awaitingApproval"}


def dump(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False), encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--cases", default="chat,catalog,delete,js,run")
    args = parser.parse_args()
    original = json.loads(args.config.read_text(encoding="utf-8-sig"))
    model = next(item for item in original["models"] if item["id"] == original["activeModel"])
    if model["protocol"] != "anthropic-messages":
        raise SystemExit("当前验收代理只支持 anthropic-messages")
    if TEMP.exists():
        raise SystemExit("验收目录已存在；先核对并清理上次任务，不能覆盖未知文件")
    TEMP.mkdir()
    process = None
    servers = []
    requests = []
    invokes = []
    user = TEMP / "User"
    reports = []
    jobs = {}
    active_case = [""]

    class Quiet(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def respond(self, value, status=200):
            body = json.dumps(value, ensure_ascii=False).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    class Proxy(Quiet):
        def do_POST(self):
            raw = self.rfile.read(int(self.headers["Content-Length"]))
            body = json.loads(raw)
            system = json.dumps(body.get("system"), ensure_ascii=False)
            requests.append({"case": active_case[0], "systemChars": len(system),
                             "fullIndexLoaded": "\"sourceRevision\"" in system and "\"items\"" in system,
                             "fullOperatorLoaded": "## 创建用户资源" in system,
                             "toolCount": len(body.get("tools", []))})
            headers = {"Content-Type": "application/json", "anthropic-version": "2023-06-01"}
            key = model.get("apiKey", "")
            auth = model.get("auth", "auto")
            if auth == "apiKey" or (auth == "auto" and key.startswith("sk-ant-")):
                headers["x-api-key"] = key
            else:
                headers["Authorization"] = "Bearer " + key
            headers.update(model.get("headers", {}))
            # 请求非流式响应，运行时仍使用实际协议解码路径；所有上下文来自实际 Supervisor。
            body["stream"] = False
            req = urllib.request.Request(model["baseUrl"].rstrip("/") + "/messages",
                                         data=json.dumps(body).encode(), headers=headers)
            try:
                with urllib.request.urlopen(req, timeout=90) as response:
                    data = response.read()
                self.respond(json.loads(data))
            except urllib.error.HTTPError as error:
                # 不把任何鉴权头、原始上游错误或凭据写到测试输出。
                self.respond({"error": {"message": f"上游模型 HTTP {error.code}"}}, error.code)
            except Exception:
                self.respond({"error": {"message": "上游模型连接失败"}}, 502)

    def contract(method):
        properties = {}
        if method == "bgi.delete_script_group":
            properties = {"groupName": {"type": "string"}, "expectedSha256": {"type": "string"}}
        elif method == "bgi.run_script_group":
            properties = {"name": {"type": "string"}}
        elif method == "bgi.inspect_local_resource":
            properties = {"path": {"type": "string"}}
        elif method == "bgi.delete_local_resource":
            properties = {"path": {"type": "string"},"expectedVersion":{"type":"string"}}
        return {"methodId": method, "catalogVersion": "fixture-v1", "instanceId": "fixture",
                "callable": method in {"bgi.delete_script_group", "bgi.run_script_group","bgi.inspect_local_resource","bgi.delete_local_resource"},
                "effect": "readOnly" if method=="bgi.inspect_local_resource" else "hostCommand", "guide": {"purpose": "隔离验证入口：按路径检查／删除路线资源，不依赖 UI" if "local_resource" in method else "隔离验证入口", "resultMeaning": "verified 表示本项完成"},
                "inputSchema": {"type": "object", "properties": properties,
                                "required": list(properties), "additionalProperties": False},
                "outputSchema": {"type": "object"}}

    class Host(Quiet):
        def do_GET(self):
            path = urllib.parse.urlparse(self.path).path
            if path.endswith("/info"):
                self.respond({"protocolVersion": "1", "instanceId": "fixture", "catalogVersion": "fixture-v1", "features": ["jobs"]})
            elif path.endswith("/host"):
                self.respond({"userPath": str(user), "installPath": str(TEMP)})
            elif path.endswith("/state"):
                self.respond({"instanceId": "fixture", "observedAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                              "snapshotId": "fixture", "runtime": {"captureReady": True, "inMainUi": True, "taskRunning": False}})
            elif "/catalog/" in path:
                self.respond(contract(urllib.parse.unquote(path.rsplit("/", 1)[-1])))
            elif "/jobs/" in path:
                self.respond(jobs[path.rsplit("/", 1)[-1]])
            elif path.endswith("/catalog"):
                self.respond({"total": 4, "items": [contract(name) for name in ("bgi.delete_script_group","bgi.run_script_group","bgi.inspect_local_resource","bgi.delete_local_resource")]})
            else:
                self.respond({"message": "验收未提供此接口"}, 404)

        def do_POST(self):
            value = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            method = value.get("methodId")
            invokes.append({"case": active_case[0], "method": method, "arguments": value.get("arguments")})
            result = {}
            if method == "bgi.delete_script_group":
                target = user / "ScriptGroup" / (value["arguments"]["groupName"] + ".json")
                if hashlib.sha256(target.read_bytes()).hexdigest() != value["arguments"]["expectedSha256"]:
                    self.respond({"message": "版本不符"}, 409)
                    return
                backup = target.with_suffix(".json.sleepy-doll.fixture.bak")
                shutil.copyfile(target, backup)
                target.unlink()
                result = {"deleted": True, "verified": True, "backup": str(backup.relative_to(user))}
            elif method=="bgi.inspect_local_resource":
                target=user/value["arguments"]["path"]
                files=sorted(str(path.relative_to(target)).replace('\\','/') for path in target.rglob('*.json'))
                self.respond({"result":{"path":value["arguments"]["path"],"version":"a"*64,"totalFiles":len(files),"files":files,"references":[],"coveringSubscriptions":[],"next":"直接 describe/invoke bgi.delete_local_resource；不要创建页面或展开 UI 树。"}})
                return
            elif method=="bgi.delete_local_resource":
                assert value["arguments"]["expectedVersion"]=="a"*64
                target=user/value["arguments"]["path"];backup=TEMP/"deleted-route-payload";target.rename(backup)
                result={"result":{"deleted":True,"verified":True,"deletedFiles":5,"backupId":"fixture-resource-backup","path":value["arguments"]["path"]}}
            elif method == "bgi.run_script_group":
                result = {"finished": True, "verified": True, "name": value["arguments"]["name"]}
            else:
                self.respond({"message": "验收拒绝未登记的执行"}, 404)
                return
            job_id = value.get("requestId", "fixture-job")
            jobs[job_id] = {"id": job_id, "state": "completed", "result": result,
                            "verification": {"status": "succeeded", "evidence": result}}
            self.respond({"jobId": job_id, "state": "accepted"})

    def start(handler):
        server = ThreadingHTTPServer(("127.0.0.1", 0), handler)
        server.daemon_threads = True
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        servers.append((server, thread))
        return f"http://127.0.0.1:{server.server_port}"

    def rpc(method, params):
        process.stdin.write(json.dumps({"method": method, "params": params}) + "\n")
        process.stdin.flush()
        value = json.loads(process.stdout.readline())
        if not value["ok"]:
            raise RuntimeError(value["error"])
        return value["result"]

    try:
        config = copy.deepcopy(original)
        selected = copy.deepcopy(model)
        selected.update(baseUrl=start(Proxy), apiKey="fixture-proxy", headers={})
        selected["options"] = {"maxOutputTokens": 2048, "timeoutMs": 100000, "contextWindow": 200000, "promptCache": False}
        config.update(version=5, models=[selected], activeModel=selected["id"], hooks=[])
        config["agent"].update(skillDirectories=[], fallbackModels=[])
        config["plugins"] = {"directories": [str(ROOT / "plugins")], "enabled": ["bgi"], "disabled": []}
        config["storage"] = {"database": str(TEMP / "data.db")}
        config["runtime"].update(catalogDirectory=str(TEMP), permissionMode="fullAccess", grants=[], trustGrants=[], maxDecisions=12, maxTools=30, durationSec=180)
        config["bridge"] = {"enabled": True, "baseUrl": start(Host), "token": "fixture-host", "timeoutMs": 5000, "hostInstallPath": None}
        dump(user / "ScriptGroup" / "Sleepy Doll-地图追踪-植绒草.json", {"name": "Sleepy Doll-地图追踪-植绒草", "index": 0, "config": {}, "projects": []})
        dump(user / "AutoPathing" / "稻妻" / "血斛.json", {"info": {"name": "血斛"}, "positions": [{"id": 1, "x": 100, "y": 100, "type": "target"}]})
        dump(user / "ScriptGroup" / "血斛.json", {"name": "血斛", "index": 1, "config": {}, "projects": [{"name": "血斛.json", "type": "Pathing", "folderName": "稻妻", "status": "Enabled"}]})
        dump(user / "JsScript" / "委托脚本" / "manifest.json", {"name": "使用历练点完成每日委托", "main": "main.js", "settings_ui": "settings.json"})
        dump(user / "JsScript" / "委托脚本" / "settings.json", [{"name": "movePartyName", "type": "input-text", "label": "移动专用队伍名称", "default": ""}])
        (user / "JsScript" / "委托脚本" / "main.js").write_text('const movePartyName = settings.movePartyName;\nif (movePartyName) { await genshin.switchParty(movePartyName); }\nawait pathingScript.run("领取奖励.json");\n', encoding="utf-8")
        dump(TEMP / "config.json", config)
        process = subprocess.Popen([str(ROOT / "target/debug/bgi-agent-check.exe"), str(TEMP / "config.json")],
                                   stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                   text=True, encoding="utf-8")
        cases = {"chat": "17加25是多少？", "catalog": "BGI目前有哪些功能，按分类简短列举一下。",
                 "delete": "帮我把植绒草那个配置组删一下，以后我不采了，留着碍事。",
                 "js": "使用历练点完成每日委托这个脚本的movePartyName有什么用？",
                 "run": "帮我跑下血斛。", "delete_resources":"血斛的配置组还有路线也删一下"}
        for name in args.cases.split(","):
            active_case[0] = name
            if name=="delete_resources":
                (user/"AutoPathing/稻妻/血斛.json").unlink()
                folder="地方特产/稻妻/血斛/血斛@固定作者包"
                for index in range(1,6):dump(user/"AutoPathing"/folder/f"路线{index}.json",{"positions":[{"id":index}]})
                dump(user/"ScriptGroup/血斛.json",{"name":"血斛","projects":[{"name":f"路线{index}.json","type":"Pathing","folderName":folder,"status":"Enabled"} for index in range(1,6)]})
            begin = len(requests)
            run = rpc("run.submit", {"prompt": cases[name], "durationSec": 180})
            end = time.monotonic() + 190
            while run["state"] not in TERMINAL and time.monotonic() < end:
                time.sleep(0.5)
                run = rpc("run.get", {"id": run["id"]})
            if run["state"] not in TERMINAL:
                rpc("run.cancel", {"id": run["id"]})
                raise RuntimeError("隔离运行超时")
            history = rpc("conversation.get", {"id": run["conversationId"]})["messages"]
            calls = [call for message in history for call in message.get("toolCalls", [])]
            evidence = {"case": name, "state": run["state"], "result": run.get("result"), "error": run.get("error"),
                        "calls": [{"name": call["name"], "arguments": call["arguments"]} for call in calls],
                        "requests": requests[begin:]}
            reports.append(evidence)
            print(json.dumps(evidence, ensure_ascii=False), flush=True)
            names = [call["name"] for call in calls]
            assert run["state"] in {"answered", "succeeded"}, evidence
            if name == "chat":
                assert re.search(r"(?<!\d)42(?!\d)", run["result"]) and not names
            elif name == "catalog":
                assert "bgi.feature.search" in names and "bgi.feature.read" not in names
            elif name == "delete":
                assert not (user / "ScriptGroup" / "Sleepy Doll-地图追踪-植绒草.json").exists()
                assert any(item["case"] == name and item["method"] == "bgi.delete_script_group" for item in invokes)
            elif name == "js":
                assert "bgi.user.inspect_script" in names
                assert any(call["name"] == "bgi.user.read" and call["arguments"]["path"].endswith("main.js") for call in calls)
                # inspect_script 已返回真实参数定义，不要求模型重复读取 settings。
            elif name == "run":
                assert names[0] == "bgi.user.resolve"
                assert any(item["case"] == name and item["method"] == "bgi.run_script_group" and item["arguments"]["name"] == "血斛" for item in invokes)
            elif name=="delete_resources":
                assert not (user/"ScriptGroup/血斛.json").exists()
                assert not (user/"AutoPathing/地方特产/稻妻/血斛/血斛@固定作者包").exists()
                assert any(item["case"]==name and item["method"]=="bgi.delete_local_resource" for item in invokes)
                assert not any(call["arguments"].get("methodId","").startswith(("bgi.ui.","cmd.map_pathing","bgi.create_command","bgi.list_command","bgi.open_page")) for call in calls)
                assert len(calls)<=18, evidence
        assert all(not req["fullIndexLoaded"] and not req["fullOperatorLoaded"] for req in requests)
        print(json.dumps({"cases": len(reports), "model": model["model"], "invokes": invokes,
                          "realBgiTouched": False, "temporaryDataCleanedOnExit": True}, ensure_ascii=False), flush=True)
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
        for server, thread in servers:
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)
        # 仅删除此调用明确创建且解析在系统临时目录下的固定目录。
        if TEMP.resolve().parent == Path(tempfile.gettempdir()).resolve():
            shutil.rmtree(TEMP)


if __name__ == "__main__":
    main()
