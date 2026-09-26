"""从宿主源码索引与只读桥目录生成 BGI 功能覆盖清单。"""
import argparse
import collections
import json
import re
import subprocess
import urllib.request
import urllib.error
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def snake(value):
    return re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", value).lower()


def camel(value):
    result = list(value)
    for index, char in enumerate(result):
        if not char.isupper() or index > 0 and index + 1 < len(result) and result[index + 1].islower():
            break
        result[index] = char.lower()
    return "".join(result)


def live_catalog(config_path, cache):
    config = json.loads(config_path.read_text(encoding="utf-8-sig"))["bridge"]
    base = config["baseUrl"].rstrip("/")
    token = config.get("token") or ""
    if token.startswith("${ENV:") and token.endswith("}"):
        import os
        token = os.environ.get(token[6:-1], "")

    def read(path):
        request = urllib.request.Request(base + "/bridge/v1/" + path,
                                         headers={"Authorization": "Bearer " + token})
        with urllib.request.urlopen(request, timeout=10) as response:
            return json.load(response)

    info = read("info")
    items = []
    offset = 0
    while True:
        result = read(f"catalog?q=&limit=50&offset={offset}")
        items.extend(result["items"])
        if result.get("nextOffset") is None:
            break
        offset = result["nextOffset"]
    contracts = []
    keep = {"methodId", "group", "summary", "displayName", "callable", "unavailableReason",
            "effect", "inputSchema", "outputSchema", "requiresGameReady", "whenToUse",
            "preconditions", "sideEffects", "resultMeaning", "verification", "rollback", "source"}
    for item in items:
        detail = read("catalog/" + item["methodId"])
        contracts.append({key: value for key, value in detail.items() if key in keep})
    safe = {"info": {key: info.get(key) for key in
                     ["protocolVersion", "bridge", "bridgeCode", "catalogVersion", "methods"]},
            "items": items, "contracts": contracts}
    cache.parent.mkdir(parents=True, exist_ok=True)
    cache.write_text(json.dumps(safe, ensure_ascii=False, indent=2), encoding="utf-8")
    return safe


def make_inventory(source, metadata, live):
    entries = metadata["entries"]
    commands = []
    script_apis = []
    properties = collections.defaultdict(list)
    for key, entry in entries.items():
        if not entry["source"].startswith("BetterGenshinImpact/"):
            continue
        symbol = key[2:]
        if key.startswith("M:"):
            head, signature = symbol.split("(", 1)
            owner, name = head.rsplit(".", 1)
            script_apis.append(dict(entry, symbol=key, owner=owner, name=name, signature=signature))
            continue
        owner, name = symbol.rsplit(".", 1)
        record = dict(entry, symbol=key, owner=owner, name=name)
        if key.startswith("C:"):
            viewmodel = owner.rsplit(".", 1)[-1].removesuffix("ViewModel")
            record["methodId"] = "cmd." + snake(viewmodel) + "." + snake(name.removesuffix("Command"))
            commands.append(record)
        elif key.startswith("P:") and not entry.get("isStatic") and not entry.get("jsonIgnore"):
            properties[owner].append(record)
    by_type = collections.defaultdict(list)
    for owner in properties:
        by_type[owner.rsplit(".", 1)[-1]].append(owner)
    settings = []
    resource_models = []
    for owner, members in properties.items():
        if owner.startswith("BetterGenshinImpact.Core.Script.Group.") or owner.startswith("BetterGenshinImpact.Core.Script.Project."):
            resource_models.extend(members)

    def walk(owner, prefix, ancestors):
        if owner in ancestors or len(ancestors) > 14:
            return
        for member in properties[owner]:
            name = member.get("jsonName") or camel(member["name"])
            path = prefix + name
            kind = (member.get("valueType") or "").removesuffix("?")
            target = by_type.get(kind, [])
            if len(target) == 1 and "<" not in kind and not kind.endswith("[]"):
                walk(target[0], path + ".", ancestors | {owner})
            else:
                settings.append(dict(member, path=path, methodId="setting." + path))
    walk("BetterGenshinImpact.Core.Config.AllConfig", "", set())
    pages = []
    for file in sorted((source / "BetterGenshinImpact/View").rglob("*.xaml")):
        xml = ET.parse(file).getroot()
        bindings = []
        for element in xml.iter():
            for key, value in element.attrib.items():
                if key.rsplit("}", 1)[-1] == "Command":
                    bindings.append({"binding": value, "label": element.attrib.get("Content") or element.attrib.get("Header") or ""})
        pages.append({"source": file.relative_to(source).as_posix(),
                      "class": xml.attrib.get("{http://schemas.microsoft.com/winfx/2006/xaml}Class"),
                      "commands": bindings,
                      "events": [{"event": key, "handler": value} for element in xml.iter() for key, value in element.attrib.items()
                                 if key in {"Click", "SelectionChanged", "Checked", "Unchecked", "Loaded", "Closing", "Drop", "PreviewKeyDown"}]})
    runtime = {item["methodId"]: item for item in live.get("items", [])}
    internal = {"Activated", "Closing", "Loaded", "WindowSizeChanged", "OverlayLayoutCommitted", "PointClick", "PointHover", "PointRightClick", "DropDownChanged", "ConfigDropDownChanged", "CaptureModeDropDownChanged"}
    alternatives = {"DeleteScriptGroupCommand": "bgi.delete_script_group", "StartRunCommand": "bgi.prepare_js_group + bgi.run_script_group",
                    "StartMultiScriptGroupCommand": "bgi.run_script_group", "SOneDragonFlowCommand": "bgi.run_one_dragon",
                    "OneKeyExecuteCommand": "bgi.run_one_dragon", "StopSoloTaskCommand": "bgi.stop_current_task"}
    supported = {"string", "bool", "int", "double", "float", "long", "decimal"} | set(metadata["enums"])
    for item in commands:
        item["publicExposure"] = not bool(item.get("exposureReason"))
        parameter = (item.get("valueType") or "").removesuffix("?")
        name = item["name"].removesuffix("Command")
        if item.get("exposureReason"):
            chain = "已移除：" + item["exposureReason"]
        elif name in internal or item.get("hasImplementation") is False:
            chain = "内部事件／空实现，不作为业务入口"
        elif item.get("asyncVoid") and item.get("hasAwait", True):
            chain = "异步不可跟踪，需稳定替代"
        elif item.get("needsDialogInput") and not name.startswith(("Open", "Show")):
            chain = "需 dialogInput 绑定本次弹窗并核验"
        elif parameter and parameter not in supported and parameter not in {"string[]", "List<string>"}:
            chain = "原生对象引用或声明类型构造，需上下文绑定"
        elif item.get("usesSelection"):
            chain = "需核对当前选择，不能仅按名称宣称完成"
        else:
            chain = "可走动态命令链，需参数与结果核验"
        item["chainAssessment"] = chain
        item["stableAlternative"] = alternatives.get(item["name"], "")
    for item in commands + settings:
        item["publicExposure"] = not bool(item.get("exposureReason"))
        if not item["publicExposure"]:
            item["bridgeState"] = "新版公共目录已移除"
            item["reason"] = item["exposureReason"]
            continue
        actual = runtime.get(item["methodId"])
        if actual:
            item["bridgeState"] = "可调用" if actual["callable"] else "当前不可调用"
            item["reason"] = actual.get("unavailableReason") or ""
        elif not runtime:
            item["bridgeState"] = "源码已列出，运行桥尚未核验"
            item["reason"] = "当前没有可连接的桥快照，不将未核验记为缺失。"
        elif item["symbol"].startswith("C:"):
            item["bridgeState"] = "源码存在，当前桥目录未登记"
            item["reason"] = "需核对宿主版本、服务注册或当前对象绑定；不能据此宣称功能不存在。"
        else:
            item["bridgeState"] = "源码设置，当前桥未匹配"
            item["reason"] = "需核对版本、父对象初始化与安全读写契约。"
    stable = []
    for file in sorted((ROOT / "bgi-bridge/managed/Tools").glob("*.cs")):
        for method in sorted(set(re.findall(r'"(bgi\.[a-z_]+)"', file.read_text(encoding="utf-8")))):
            if method.startswith("bgi."):
                stable.append({"methodId": method, "source": file.relative_to(ROOT).as_posix(),
                               "live": method in runtime})
    assessments = dict(collections.Counter(item["chainAssessment"] for item in commands))
    skill_refs = {}
    known = set(re.findall(r'"(bgi\.[a-zA-Z_][a-zA-Z0-9_.]*)"', "\n".join(file.read_text(encoding="utf-8-sig") for file in
                    list((ROOT / "bgi-bridge/managed").rglob("*.cs")) + [ROOT / "src/bridge/mod.rs"])))
    for file in (ROOT / "plugins/bgi/skills").rglob("*.md"):
        for method in set(re.findall(r"\bbgi\.[a-zA-Z_][a-zA-Z0-9_.]*", file.read_text(encoding="utf-8"))):
            if method in known:
                skill_refs.setdefault(method, []).append(file.relative_to(ROOT).as_posix())
    return {"source": str(source), "sourceRevision": subprocess.check_output(
        ["git", "-C", str(source), "rev-parse", "HEAD"], text=True).strip(),
        "commands": sorted(commands, key=lambda item: item["symbol"]),
        "settings": sorted(settings, key=lambda item: item["path"]),
        "views": pages, "stableEntries": stable, "liveInfo": live.get("info", {}),
        "scriptApis": script_apis, "resourceModels": resource_models,
        "chainAssessments": assessments,
        "skillReferences": skill_refs,
        "counts": {"commands": len(commands), "settings": len(settings), "views": len(pages),
                   "scriptApis": len(script_apis), "resourceProperties": len(resource_models),
                   "liveMethods": len(runtime), "blockedCommands": sum(item["bridgeState"] == "当前不可调用" for item in commands)}}


def cell(value):
    return str(value or "").replace("|", "\\|").replace("\r", " ").replace("\n", " ")


def render(data):
    count = data["counts"]
    lines = ["# BGI 功能与适配覆盖清单", "",
             f"源码事实源：`{data['source']}`；提交：`{data['sourceRevision']}`。", "",
             f"全量索引：{count['commands']} 个源码命令、{count['settings']} 个 AllConfig 设置叶节点、{count['views']} 个 XAML 页面／窗口、{count['scriptApis']} 个脚本 API 方法、{count['resourceProperties']} 个脚本资源模型字段；只读快照中有 {count['liveMethods']} 个桥接口。", "",
             "本清单区分源码功能、当前运行桥的登记结果与实际验证。可调用只表示契约可提交，不代表游戏任务已完成；当前桥未登记可能是版本差异、对象未注册或真实缺口。源码中的内部事件和编辑器命令也列出，不将它们包装成普通自动化能力。", "",
             "审计判断与修复记录见 [链路审计](integration-audit.md)。完整结构化数据见 [feature-coverage.json](feature-coverage.json)。", "",
             "## 源码命令", "",
             "源码链路分类（非实机通过数量）：`" + json.dumps(data["chainAssessments"], ensure_ascii=False) + "`。", "",
             "| 功能／界面标签 | 源码对象与位置 | 参数 | 桥入口／状态 | 输入与选择依赖 | 链路审查／替代 |",
             "|---|---|---|---|---|---|"]
    for item in data["commands"]:
        title = item.get("label") or item.get("summary") or item["name"]
        parameters = ", ".join(p["type"] + " " + p["name"] for p in item.get("parameters") or [])
        dependencies = ("弹窗输入；" if item.get("needsDialogInput") else "") + ("当前选择；" if item.get("usesSelection") else "") + item["reason"]
        lines.append("| " + " | ".join(map(cell, [title, f"{item['owner']} / {item['source']}:{item['line']}", parameters or "无参数", f"{item['methodId']} / {item['bridgeState']}", dependencies, item["chainAssessment"] + ("；" + item["stableAlternative"] if item["stableAlternative"] else "")])) + " |")
    lines += ["", "## 全局设置", "", "| 设置路径 | 含义 | 类型 | 源码位置 | 桥状态／联动 |", "|---|---|---|---|---|"]
    for item in data["settings"]:
        restriction = item["bridgeState"] + "；" + item["reason"] + ("；有联动变更钩子" if item.get("hasCustomChangeHook") else "")
        lines.append("| " + " | ".join(map(cell, [item["path"], item.get("summary"), item.get("valueType"), f"{item['source']}:{item['line']}", restriction])) + " |")
    lines += ["", "## 页面与窗口", "", "| 页面／窗口源码 | 绑定命令 |", "|---|---|"]
    for item in data["views"]:
        bindings = "；".join(c["binding"] for c in item["commands"])
        events = "；".join(event["event"] + "=" + event["handler"] for event in item["events"])
        lines.append("| " + cell(item["source"]) + " | " + cell(bindings + ("；代码后置事件：" + events if events else "")) + " |")
    lines += ["", "## 脚本资源模型字段", "", "| 模型字段 | 类型 | 说明 | 源码位置 |", "|---|---|---|---|"]
    for item in data["resourceModels"]:
        lines.append("| " + " | ".join(map(cell, [item["owner"] + "." + item["name"], item["valueType"], item["summary"], f"{item['source']}:{item['line']}"])) + " |")
    lines += ["", "## JavaScript 宿主 API", "", "这些是脚本运行环境中的 API，不等同于 Agent 已有直接桥入口；用 bgi-javascript 追踪脚本调用。", "", "| 方法 | 参数 | 返回 | 说明／源码 |", "|---|---|---|---|"]
    for item in data["scriptApis"]:
        parameters = ", ".join(p["type"] + " " + p["name"] for p in item.get("parameters") or [])
        lines.append("| " + " | ".join(map(cell, [item["owner"] + "." + item["name"], parameters, item["valueType"], f"{item['summary']} / {item['source']}:{item['line']}"])) + " |")
    lines += ["", "## 稳定 BGI 操作入口", "", "| 入口 | 桥实现 | 当前运行桥是否登记 |", "|---|---|---|"]
    for item in data["stableEntries"]:
        lines.append("| " + " | ".join(map(cell, [item["methodId"], item["source"], "是" if item["live"] else "否／需核对更新版本"])) + " |")
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("--live-config", type=Path)
    args = parser.parse_args()
    cache = ROOT / "target/bgi-coverage/catalog.json"
    live = {"items": []}
    if args.live_config:
        try:
            live = live_catalog(args.live_config, cache)
        except urllib.error.URLError as error:
            print("当前桥只读快照不可用，继续源码审计：" + str(error.reason))
    elif cache.exists():
        live = json.loads(cache.read_text(encoding="utf-8"))
    metadata = json.loads((ROOT / "bgi-bridge/managed/generated/host-documentation.json").read_text(encoding="utf-8"))
    data = make_inventory(args.source.resolve(), metadata, live)
    output = ROOT / "docs/bgi"
    (output / "feature-coverage.json").write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")
    (output / "feature-coverage.md").write_text(render(data), encoding="utf-8")
    print(json.dumps(data["counts"], ensure_ascii=False))


if __name__ == "__main__":
    main()
