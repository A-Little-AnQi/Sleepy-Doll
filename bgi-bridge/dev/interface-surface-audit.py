"""Compare source declarations, the generated index and a read-only runtime snapshot."""
import argparse
import collections
import datetime
import hashlib
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def load(path):
    return json.loads(path.read_text(encoding="utf-8-sig"))


def content_bytes(path):
    return path.read_text(encoding="utf-8-sig").encode("utf-8")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--live-snapshot", type=Path, required=True)
    parser.add_argument("--before-index", type=Path, required=True)
    parser.add_argument("--runtime-offline", action="store_true", help="本轮后续只读探测未连上宿主；不启动程序")
    args = parser.parse_args()
    surface = load(ROOT / "bgi-bridge/dev/fixtures/host-surface.json")
    source = load(ROOT / "bgi-bridge/dev/fixtures/host-features.json")
    documentation_path = ROOT / "bgi-bridge/managed/generated/host-documentation.json"
    documentation = load(documentation_path)
    index = load(ROOT / "plugins/bgi/resources/feature-index.json")
    registrations = load(ROOT / "bgi-bridge/dev/bgi-method-metadata.json")
    before_input, live_input = load(args.before_index), load(args.live_snapshot)
    before = before_input.get("baselineIndex", before_input)
    live = live_input.get("catalogSnapshot", live_input)
    for field in ["sourceRevision", "sourceTreeHash"]:
        assert surface[field] == source[field] == documentation[field] == index[field], field
    assert index["sourceSnapshotHash"] == hashlib.sha256(content_bytes(documentation_path) + b"\0" + content_bytes(ROOT / "bgi-bridge/dev/fixtures/host-surface.json")).hexdigest()
    inputs = [ROOT / "bgi-bridge/dev/fixtures/host-features.json", ROOT / "bgi-bridge/dev/bgi-method-metadata.json",
              documentation_path, ROOT / "bgi-bridge/dev/fixtures/host-surface.json"]
    assert index["indexInputsHash"] == hashlib.sha256(b"".join(hashlib.sha256(content_bytes(path)).digest() for path in inputs)).hexdigest()
    assert registrations.keys() <= {item["id"] for item in index["items"]}, "Compiled registration absent from index"
    subprocess.run(["dotnet", "run", "--project", str(ROOT / "bgi-bridge/dev/ProductionCoreChecks.csproj"),
                    "--no-launch-profile", "-p:UseSharedCompilation=false", "--", "--check-interfaces",
                    str(ROOT / "bgi-bridge/dev/bgi-method-metadata.json")], cwd=ROOT, check=True)
    current_ids, old_ids = {item["id"] for item in index["items"]}, set(before["ids"])
    native = {item["methodId"]: item for item in live["items"]}
    commands = {item["owner"] + "." + item["name"].removesuffix("Command"): item for item in source["commands"]}
    members = []
    for method in surface["methods"].values():
        if method["kind"] == "command":
            name = method["name"]
            if name.startswith("On") and len(name) > 2 and name[2].isupper(): name = name[2:]
            command = commands.get(method["owner"] + "." + name.removesuffix("Async"))
            state = "commandMappingNeedsReview" if not command else "excludedByExistingRule" if not command["publicExposure"] else "sourceCommand"
            route = command["methodId"] if command else None
            reason = command.get("exposureReason") if command else None
        elif method["owner"] in documentation["scriptTypes"]:
            state, route, reason = "scriptSourceReachable", "bgi.js_api.read -> native JS execution", None
        else:
            state, route, reason = "sourceDeclarationOnly", None, None
        members.append({**method, "coverageState": state, "route": route, "exclusionReason": reason,
                        "runtimeExecutionVerified": False})

    def evidence(owner, names):
        result = [{key: item[key] for key in ["id", "source", "line", "name", "returnType"]}
                  for item in surface["methods"].values() if item["owner"] == owner and item["name"] in names]
        assert len(result) == len(names), owner
        return result

    findings = [
        {"id": "redeem-data", "state": "adapterGap", "title": "兑换码缺少独立的无界面结构化取码入口",
         "evidence": evidence("BetterGenshinImpact.GameTask.UseRedeemCode.GetLiveRedeemCode", ["GetCodeMsgAsync"]),
         "existingRoute": ["cmd.feed_window.get_live_redeem_codes", "bgi.create_command_target"],
         "gap": "现有窗口命令更新 ViewModel 并用 Toast 报错，未直接返回取码结果。底层异步取码方法存在；普通调用可创建独立上下文，可靠定时计划仍缺运行时构造步骤。"},
        {"id": "map-data", "state": "adapterGap", "title": "米游社及 HoYoLAB 地图服务缺少独立数据适配",
         "evidence": evidence("BetterGenshinImpact.Service.MihoyoMapApiService", ["GetLabelTreeAsync", "GetPointListAsync", "GetPointInfoAsync"])
            + evidence("BetterGenshinImpact.Service.HoYoLabMapApiService", ["GetLabelTreeAsync", "GetPointListAsync", "GetPointInfoAsync"]),
         "existingRoute": ["相关原生地图界面及命令"],
         "gap": "源码提供标签树、点位列表和详情查询，但本轮桥目录没有对应的独立数据调用；界面可操作不能替代有分页、错误与返回值契约的数据接口。"},
        {"id": "tavern-data", "state": "adapterGap", "title": "空荧酒馆数据查询仍依赖内部服务链",
         "evidence": evidence("BetterGenshinImpact.Service.Tavern.KongyingTavernApiService", ["GetIconListAsync", "GetItemTypeListAsync", "GetMarkerListAsync"]),
         "existingRoute": ["遮罩地图点位相关界面"],
         "gap": "原生服务已有图标、物品类别、标记列表查询，未登记独立的结构化查询入口；不能把底层令牌方法或任意 URL 调用直接开放来代替适配。"},
        {"id": "plan-context", "state": "compositionGap", "title": "普通命令上下文创建没有接进可靠定时计划",
         "evidence": [{"source":"bgi-bridge/managed/Tools/CommandPlanTools.cs", "symbol":"NativePlanHost.Owner"},
                      {"source":"bgi-bridge/managed/Bgi/CommandPlan.cs", "symbol":"Schema / Run"}],
         "existingRoute": ["bgi.create_command_target", "bgi.run_host_command_plan", "bgi.run_game_command_plan"],
         "gap": "当前计划使用唯一既有上下文或本次命令同步创建的上下文，缺少独立的运行时构造与回收步骤。创建时拿到的临时引用不能保存给未来使用。"},
        {"id": "command-results", "state": "verificationGap", "title": "命令处理器完成仍不等于业务数据或业务结果已交付",
         "evidence": [{"source":"bgi-bridge/managed/Bgi/CommandPlan.cs", "symbol":"Run"}],
         "existingRoute": ["当前命令计划的 observe / 原生业务回执"],
         "gap": "已有局部数据观察和兑换日志核验，但没有统一闭合所有任务的返回、等待、取消与业务回读。没有逐项证据的能力不能记为执行已验收。"},
    ]
    live_entries = [{"methodId": key, "group": item["group"], "snapshotCallable": item.get("callable"),
                     "indexedBefore": key in old_ids, "indexedAfter": key in current_ids,
                     "businessExecutionVerified": False} for key, item in sorted(native.items())]
    missing_before = [entry["methodId"] for entry in live_entries if not entry["indexedBefore"]]
    missing_after = [entry["methodId"] for entry in live_entries if not entry["indexedAfter"]]
    observed = live_input.get("runtimeSnapshot", {}).get("observedAtUtc") or datetime.datetime.fromtimestamp(args.live_snapshot.stat().st_mtime, datetime.timezone.utc).isoformat()
    data = {"format": 1, "sourceRevision": surface["sourceRevision"], "sourceTreeHash": surface["sourceTreeHash"],
            "sourceScope": surface["sourceScope"], "sourceDirty": surface["sourceDirty"],
            "runtimeSnapshot": {**live["info"], "observedAtUtc": observed, "sourceRevisionMatchVerified": False,
                                "currentConnection": "offlineAtLastProbe" if args.runtime_offline else "notReprobed",
                                "note": "保存的只读目录快照，不作为当前连接或全部业务执行证明。"},
            "counts": {"runtimeEntries": len(native), "compiledStableRegistrations": len(registrations),
                       "sourceTypes": len(surface["types"]), "sourceMethods": len(members),
                       "indexBefore": before["counts"], "indexAfter": dict(collections.Counter(item["kind"] for item in index["items"])),
                       "runtimeUnindexedBefore": len(missing_before), "runtimeUnindexedAfter": len(missing_after),
                       "sourceMethodStates": dict(collections.Counter(item["coverageState"] for item in members))},
            "runtimeUnindexedBefore": missing_before, "runtimeUnindexedAfter": missing_after,
            "findings": findings, "runtimeEntries": live_entries, "sourceMethods": members,
            "baselineIndex": before, "catalogSnapshot": live,
            "completion": "发现链和生成一致性已修；业务适配、可组合定时及逐项执行验收尚未全部闭合，不能宣称全部能力完成。"}
    output = ROOT / "target/reports/bgi/interface-audit.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")
    lines = ["# BGI 能力接口复查", "", "[完整结构化数据](interface-audit.json)", "", "## 结论", "", data["completion"], "",
             "源码方法数量是检查范围，不是遗漏功能数量。源码可见、目录登记、参数可构造、可组合执行、业务结果可核验分别记录。", "",
             f"源码提交：`{surface['sourceRevision']}`；代码／界面声明指纹：`{surface['sourceTreeHash']}`。", "",
             f"本轮只读快照 {len(native)} 条目录，当前编译稳定接口 {len(registrations)} 项。原离线索引漏收快照中的 {len(missing_before)} 项；刷新和修正后剩 {len(missing_after)} 项需核对宿主版本或运行时派生成员，不能直接当成业务功能不存在。", "",
             f"源码范围：{len(surface['types'])} 个类型、{len(members)} 个显式命令／公共方法声明；不包含未引用测试工程。宿主版本与源码提交一致性未得到证明，未执行全部游戏、账号、通知或资源修改。", "",
             "运行目录是本轮保存的只读快照；当前连接未重核或已离线，不自动启动程序。", "",
             "## 确认的调用链缺口", ""]
    for finding in findings:
        lines += ["### " + finding["title"], "", finding["gap"], "",
                  "已有路径：" + "、".join("`" + route + "`" for route in finding["existingRoute"]) + "。", ""]
    lines += ["## 仍未进入源码索引的运行时记录", "",
              "这些项目已经在本轮运行快照中登记；目前需区分版本差异、继承／计算成员和公开范围，不通过猜测补造源码声明。", ""]
    lines += ["- `" + name + "`" for name in missing_after]
    lines += ["", "## 运行时目录逐项对照", "", "| 目录项 | 原索引 | 新索引 | 实际业务执行 |", "| --- | --- | --- | --- |"]
    lines += [f"| `{entry['methodId']}` | {'有' if entry['indexedBefore'] else '未收录'} | {'有' if entry['indexedAfter'] else '需核对'} | 未逐项验证 |" for entry in live_entries]
    lines += ["", "## 源码声明逐项审计", "", "下列 sourceDeclarationOnly 仅表示尚无逐项映射证据，不等于每个工具、DTO、框架函数都要独立发布。内部生命周期和诊断也不能冒充产品能力。", "",
              "| 源码成员 | 当前证据 | 路径或排除规则 |", "| --- | --- | --- |"]
    lines += [f"| `{item['id'].replace('|','&#124;')}` | {item['coverageState']} | {item['exclusionReason'] or item['route'] or item['source'] + ':' + str(item['line'])} |" for item in members]
    output.with_suffix(".md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(json.dumps(data["counts"], ensure_ascii=False))


if __name__ == "__main__":
    main()
