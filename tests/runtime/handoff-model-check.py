"""Validate the BGI handoff policy with a configured model and synthetic tool results.

One response per case; returned tool calls are never executed. No BGI, user DB,
or config writes. Credentials and endpoint are never printed.
"""
import argparse
import json
import os
from pathlib import Path
import re
import sys
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[2]


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser()
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--case", choices=["launch_group","all"],default="all")
    args = parser.parse_args()
    config = json.loads(args.config.read_text(encoding="utf-8-sig"))
    model = next(item for item in config["models"] if item["id"] == config["activeModel"])
    if model["protocol"] != "anthropic-messages":
        raise SystemExit("This probe uses the configured Anthropic-compatible protocol.")
    key = model.get("apiKey", "")
    match = re.fullmatch(r"\$\{ENV:([^}]+)\}", key)
    if match:
        key = os.environ.get(match[1], "")
    if not key:
        raise SystemExit("Configured credential is unavailable; no request made.")
    headers = {"Content-Type": "application/json", "anthropic-version": "2023-06-01", **model.get("headers", {})}
    auth = model.get("auth", "auto")
    if auth == "bearer" or (auth == "auto" and not key.startswith("sk-ant-")):
        headers.setdefault("Authorization", "Bearer " + key)
    else:
        headers.setdefault("x-api-key", key)
    runtime = (ROOT / "src/runtime/mod.rs").read_text(encoding="utf-8")
    core = re.search(r'const CORE_AGENT_POLICY: &str = r#"(.*?)"#;', runtime, re.S)[1]
    skill = (ROOT / "plugins/bgi/skills/bgi-assistant/SKILL.md").read_text(encoding="utf-8").split("---", 2)[-1]
    tool_source = (ROOT / "src/bridge/mod.rs").read_text(encoding="utf-8")
    invoke_description = re.search(r'\("bgi.api.invoke", "执行 BetterGI 操作", "([^"]+)"', tool_source)[1]
    tools = [
        {"name": "bgi.api.invoke", "description": invoke_description, "input_schema": {"type": "object", "properties": {"methodId": {"type": "string"}, "arguments": {"type": "object"}}, "required": ["methodId", "arguments"]}},
        {"name": "bgi.api.read", "description": "读取当前 BetterGI 状态或日志", "input_schema": {"type": "object", "properties": {"methodId": {"type": "string"}, "arguments": {"type": "object"}}}},
        {"name": "bgi.job.get", "description": "读取桥 Job 当前状态", "input_schema": {"type": "object", "properties": {"jobId": {"type": "string"}}}},
    ]
    cases = [
        ("launch_group", "跑下千星脚本", "bgi.run_script_group", {"groupName": "千星"}, {"groupName": "千星", "resolved": True, "accepted":True,"executed":False,"executionMode":"launch","verificationScope":"launch"}, False),
        ("accepted_task", "启动伐木任务", "cmd.wood.start", {}, {"started": True, "taskRunning": True}, False),
        ("explicit_followup", "跑个血斛，结束后检查日志有没有路线失败", "bgi.run_script_group", {"groupName": "血斛"}, {"groupName": "血斛", "resolved": True, "executed": True}, True),
    ]
    if args.case != "all": cases=[case for case in cases if case[0]==args.case]
    usage_total = 0
    for name, prompt, method, arguments, result, allow_calls in cases:
        evidence = {"ok": True, "value": {"jobId": "synthetic-job", "outcome": "verifiedSucceeded", "evidence": {"state": "completed", "result": result}}}
        body = {"model": model["model"], "max_tokens": 2048, "system": core + "\n\n" + skill,
                "tools": tools, "messages": [
                    {"role": "user", "content": prompt},
                    {"role": "assistant", "content": [{"type": "tool_use", "id": "synthetic-call", "name": "bgi.api.invoke", "input": {"methodId": method, "arguments": arguments}}]},
                    {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "synthetic-call", "content": json.dumps(evidence, ensure_ascii=False)}]},
                ]}
        request = urllib.request.Request(model["baseUrl"].rstrip("/") + "/messages", data=json.dumps(body).encode("utf-8"), headers=headers, method="POST")
        try:
            with urllib.request.urlopen(request, timeout=60) as response:
                output = json.load(response)
        except urllib.error.HTTPError as error:
            raise SystemExit(f"Model probe returned HTTP {error.code}; no tools executed.") from None
        except urllib.error.URLError:
            raise SystemExit("Model probe connection failed; no tools executed.") from None
        calls = [item for item in output.get("content", []) if item.get("type") == "tool_use"]
        text = "".join(item.get("text", "") for item in output.get("content", []) if item.get("type") == "text")
        usage = output.get("usage", {})
        usage_total += usage.get("input_tokens", 0) + usage.get("output_tokens", 0)
        if allow_calls:
            assert calls, f"{name}: explicit follow-up was discarded"
        else:
            assert not calls and text and output.get("stop_reason") != "max_tokens", f"{name}: model continued or failed to provide a final answer"
            if name=="launch_group": assert not re.search(r"resolved|accepted|verificationScope|waitForCompletion|executionMode",text), "Implementation fields leaked into ordinary launch reply"
        print(json.dumps({"case": name, "toolCalls": len(calls), "finalText": text[:160], "stopReason": output.get("stop_reason")}, ensure_ascii=False), flush=True)
    print(json.dumps({"passed": len(cases), "modelRequests": len(cases), "reportedTokens": usage_total, "realBgiActions": 0, "userDataModified": False}))


if __name__ == "__main__":
    main()
