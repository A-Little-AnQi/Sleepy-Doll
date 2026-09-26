"""把源码覆盖清单转成插件内的单项功能卡，供渐进式检索。"""
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
AUDIT = ROOT / "docs/bgi/feature-coverage.json"
OUTPUT = ROOT / "plugins/bgi/resources/feature-index.json"


def ref(path, skill="bgi-operator"):
    return {"skill": skill, "path": "references/" + path}


def workflow(key, title, nouns, actions, steps, proof, refs, methods=(), branches=()):
    return {"id": "workflow." + key, "kind": "workflow", "title": title,
            "keywords": nouns + actions, "nouns": nouns, "actions": actions,
            "summary": title, "methodIds": list(methods), "inputSources": ["当前用户目标与已有证据；参数从资源或当前契约读取"],
            "steps": steps, "branches": list(branches), "verification": proof,
            "references": refs, "availability": "当前契约决定是否可执行"}


WORKFLOWS = [
    workflow("resource.run", "查找并运行材料、路线或脚本", ["材料", "路线", "采集", "脚本", "特产"], ["执行", "运行", "跑", "采集", "收集", "刷"],
             ["bgi.user.resolve 传目标原话，查本机，未命中继续当前仓库", "run 直接运行；repair 只修 missing；create/resourceFound 按资源类型准备", "地图追踪选择完整父目录或作者包，JS 按 manifest.settings_ui 读取参数", "必要时订阅并 prepare_pathing_group/prepare_js_group，游戏就绪后 run_script_group"],
             "invoke 跟踪终态并读取契约要求的日志／状态；准备或启动成功不等于采集完成", [ref("execution.md"), ref("collection-workflow.md", "bgi-assistant")],
             ["bgi.user.resolve", "bgi.subscribe_script_resources", "bgi.prepare_pathing_group", "bgi.prepare_js_group", "bgi.run_script_group"],
             ["本机没有不代表仓库没有；仅当前全仓无结果才核对名称或刷新一次", "lookupFailed 是查询失败，不是资源不存在", "角色前提无法确认且无普通替代包时才询问"]),
    workflow("group.delete", "按名称删除配置组并保留路线", ["配置组", "组", "调度器"], ["删除", "删", "移除", "清理"],
             ["bgi.user.list/read 定位唯一文件，读取 name 和 sha256", "describe bgi.delete_script_group", "invoke groupName/expectedSha256；保留 backup"],
             "deleted=true 且 verified=true；目标文件与列表移除，脚本、路线、订阅保留", [ref("resources.md")], ["bgi.delete_script_group"],
             ["版本冲突重新读；同名无法唯一定位才问", "已有任务运行时报告冲突，不擅自停止", "不要改成禁用，不要求界面选中"]),
    workflow("group.edit", "创建、修改、复制或重命名用户配置组", ["配置组", "任务", "配队", "顺序", "组"], ["创建", "添加", "修改", "改", "复制", "重命名", "禁用", "启用"],
             ["list/read 取得完整目标或同类结构，JS 参数来自脚本定义", "user.write 保留未知字段，已有文件比较 sha256", "重命名先创建新 name/path 并验证，再按版本删除旧组；保留原资源和备份"],
             "回读名称、项目数、引用、关键参数和 sha256；修改不表示执行", [ref("resources.md")], ["bgi.user.read", "bgi.user.write", "bgi.delete_script_group"]),
    workflow("settings", "查询或修改全局设置", ["设置", "开关", "拾取", "剧情", "传送", "热键", "按键", "缩放", "颜色", "准星", "通知", "OCR"], ["查询", "查看", "修改", "改", "打开", "关闭", "开启", "调整"],
             ["按具体字段搜索 setting 条目或当前 settings 目录", "describe/read 获取 currentValue、valueSchema、writable、valueVersion", "单字段 set_setting，多字段 preview/commit；联动 differences 一并核对"],
             "verified 及回读值；保存 changeId，回退前比较版本", [ref("settings.md")], ["bgi.set_setting", "bgi.preview_settings", "bgi.commit_settings"],
             ["writable=false 是字段限制，不能用 workspace 改全局配置绕过", "遮蔽值不是新值，不提交 REDACTED", "当前权限由运行时处理，不重复问许可"]),
    workflow("task.run", "按参数运行独立任务", ["首领", "秘境", "地脉", "钓鱼", "伐木", "烹饪", "战斗", "七圣", "音游", "幽境", "分解", "兑换码"], ["运行", "执行", "刷", "开始", "打", "启动"],
             ["从功能条目和当前 settings 读取任务目标、次数、队伍、策略与领奖条件", "只改目标需要的参数；次数还要核对指定次数模式", "describe 最相关任务命令；检查真实游戏前置后 invoke"],
             "Job 终态加任务日志或产物；处理器返回不能证明游戏目标完成", [ref("execution.md"), ref("settings.md"), ref("action-policy.md", "bgi-assistant")],
             branches=["明确次数／奖励约束从用户目标取得；有限资源消耗不擅自增加", "脚本标题命中时先走资源执行，不归为同名本体任务"]),
    workflow("task.stop", "停止当前宿主任务或原 Job", ["任务", "脚本", "路线", "一条龙", "刚才", "运行"], ["停止", "停", "取消", "急停", "终止"],
             ["有原 Job ID 用 job.cancel 并跟踪原 Job", "无 Job ID describe/invoke stop_current_task", "音乐、录制与外部绑定使用各自停止入口"],
             "宿主 stopped=true 或原 Job 终态；timeout 和 cancellationRequested 不表示已停止", [ref("execution.md")], ["bgi.stop_current_task", "bgi.job.cancel"]),
    workflow("navigation", "打开或切换 BetterGI 页面", ["页面", "界面", "调度器", "地图追踪", "一条龙", "快捷键", "通知", "通用设置", "启动页", "独立任务", "音乐", "JS"], ["打开", "显示", "切换", "去", "进入"],
             ["describe/read list_pages 获取精确 page 标识", "describe/invoke open_page，传实际标识"],
             "opened=true、verified=true；只证明页面打开，运行／删除目标还需继续", [ref("viewmodel-usage.md", "bgi-assistant")], ["bgi.list_pages", "bgi.open_page"]),
    workflow("javascript", "阅读 JS 参数、源码与机制", ["脚本", "参数", "字段", "源码", "README", "JS", "manifest", "settings"], ["什么意思", "什么用", "作用", "解释", "分析", "为什么", "理解"],
             ["本机目标 inspect_script/read；仓库目标 repo.search/read", "先 manifest.settings_ui/main，再按字段定位分支与模块", "读取必要范围，截断继续分页并比较 sha256"],
             "解释有真实定义、分支与调用顺序依据，不从字段名猜用途", [ref("script-analysis.md"), {"skill": "bgi-javascript", "tool": "skills.read"}], ["bgi.user.inspect_script", "bgi.repo.search", "bgi.repo.read"]),
    workflow("repository", "更新仓库、订阅或导入资源", ["仓库", "订阅", "脚本", "路线", "资源"], ["更新", "同步", "刷新", "订阅", "安装", "导入"],
             ["明确 repositoryOnly、selected 或 all；路径从索引／Subscriptions 取得", "describe/invoke update_subscribed_scripts 或 subscribe_script_resources", "回读目标版本或文件，不把打开仓库窗口当更新"],
             "宿主结果和资源版本／路径回读；不启动游戏", [ref("repository.md")], ["bgi.update_subscribed_scripts", "bgi.subscribe_script_resources"]),
    workflow("logs", "定位脚本错误与宿主日志", ["脚本", "任务", "日志", "报错", "失败", "异常"], ["查", "排查", "为什么", "分析", "修", "报错", "失败"],
             ["describe/read get_script_errors，按错误记录的文件和线程定位", "user.read/repo.read 阅读相关参数与源码；需要过程时 read_host_log 过滤线程"],
             "结论对应报错原文、脚本／文件位置和具体下一步，不让用户复制可读取日志", [ref("script-analysis.md")], ["bgi.get_script_errors", "bgi.read_host_log"]),
    workflow("one-dragon", "按配置执行一条龙", ["一条龙", "每日", "日常", "清体力", "委托"], ["执行", "运行", "开始", "做", "清"],
             ["读取 User/OneDragon 的实际配置、启用任务和收尾动作", "确认次数、领奖和有限资源约束；describe run_one_dragon", "游戏就绪后 invoke 精确 configName"],
             "实际选择对应任务列表；Job 终态和一条龙日志、收尾状态", [ref("execution.md"), ref("action-policy.md", "bgi-assistant")], ["bgi.run_one_dragon"]),
    workflow("game-ready", "启动、等待或关闭游戏", ["原神", "游戏", "截图器", "分辨率", "主界面", "加载"], ["启动", "开始", "等待", "进入", "关闭", "退出", "调整"],
             ["读取当前状态与显示尺寸，不复用旧句柄", "必要时设置可容纳的目标分辨率，start_game/wait_ready", "退出游戏使用 exit_game，不关闭 BGI 或本工具"],
             "ready=true 和真实 16:9；退出回读进程／句柄状态", [ref("execution.md")], ["bgi.start_game", "bgi.wait_ready", "bgi.exit_game", "bgi.set_game_resolution"]),
    workflow("music", "音乐、曲谱与播放控制", ["音乐", "曲谱", "乐曲", "播放", "暂停"], ["播放", "停止", "暂停", "选择", "切换", "打开"],
             ["定位曲谱或当前轨道，搜索具体音乐命令", "读取参数与选择依赖，播放／暂停／停止分别调用正确命令"],
             "核对播放器状态与目标轨道，不套独立任务锁结论", [ref("viewmodel-usage.md", "bgi-assistant")]),
    workflow("notifications", "通知设置、测试与外部绑定", ["通知", "Webhook", "QQ", "微信", "绑定", "渠道"], ["设置", "配置", "测试", "绑定", "取消"],
             ["通过设置事务读取与修改用户授权渠道，敏感值保持遮蔽", "describe 当前测试或绑定命令，区分配置写入与外部验证码交互"],
             "渠道测试结果或绑定状态；外部验证码／邀请需要真实用户交互时说明唯一缺项", [ref("settings.md"), ref("action-policy.md", "bgi-assistant")]),
    workflow("editor", "录制、编辑器、点位与开发功能", ["录制", "模板", "编辑器", "点位", "拖拽", "开发", "遮罩布局"], ["编辑", "录制", "制作", "修改", "打开", "导出"],
             ["先读具体功能卡，区分业务目标与内部 WPF 输入事件", "参数／文件可提供时走受支持入口；不能编造点位或重放内部事件"],
             "保存文件、模板或编辑结果；仅窗口出现不能证明制作完成", [ref("capability-boundaries.md", "bgi-assistant"), ref("viewmodel-usage.md", "bgi-assistant")]),
    workflow("child-session", "桌面分身与子会话", ["分身", "子会话", "多开", "远程桌面"], ["启动", "停止", "创建", "设置", "连接"],
             ["检索当前子会话入口，核对 Windows 会话、权限和实际状态", "只提交明确目标的动作，不改其他账号或会话"],
             "子会话状态与实际窗口；缺环境证据时不宣称后台运行已验收", [ref("viewmodel-usage.md", "bgi-assistant")]),
]


def build(data):
    result = list(WORKFLOWS)
    revision = data["sourceRevision"]
    published_names = {item["name"] for item in data["commands"] if item.get("publicExposure", True)}
    for item in data["commands"]:
        if not item.get("publicExposure", True):
            continue
        assessment = item["chainAssessment"]
        alternative = item.get("stableAlternative", "")
        result.append({"id": item["methodId"], "kind": "command", "title": item.get("label") or item.get("summary") or item["name"],
                       "summary": item.get("summary") or item["name"], "keywords": [item["name"], item["owner"].split(".")[-1]],
                       "availability": assessment, "methodIds": [item["methodId"]], "inputSources": item.get("parameters") or [],
                       "dependencies": {key: item[key] for key in ["needsDialogInput", "usesSelection", "asyncVoid", "hasImplementation"]},
                       "steps": ["先 describe 当前 methodId，不把源码索引当作已注册契约", "对象／泛型／多实例用 list_command_targets；必要时按契约 create_command_target，参数引用真实 objectId", "按 selectionSchema 绑定目标；需要弹窗时传 dialogInput，具体参数不猜测", "核对游戏前置和实际副作用后 invoke，回读业务证据；释放桥创建的无用上下文"],
                       "branches": [assessment] + (["稳定替代：" + alternative] if alternative else []),
                       "verification": "按当前契约 verification 读取目标状态／文件／日志；async void、弹窗或内部事件不能凭处理器返回报完成",
                       "references": [ref("execution.md"), ref("viewmodel-usage.md", "bgi-assistant")],
                       "source": {"path": item["source"], "line": item["line"]}, "sourceRevision": revision})
    for item in data["settings"]:
        if not item.get("publicExposure", True):
            continue
        result.append({"id": item["methodId"], "kind": "setting", "title": item["summary"] or item["path"], "summary": item["summary"],
                       "keywords": [item["path"], item["name"], item["owner"].split(".")[-1]], "availability": "当前 writable、valueSchema 和联动事务决定",
                       "methodIds": [item["methodId"], "bgi.set_setting", "bgi.preview_settings", "bgi.commit_settings"],
                       "inputSources": [{"path": item["path"], "type": item["valueType"], "hasChangeHook": item["hasCustomChangeHook"], "sourceDefault": item.get("initial"), "sourceRange": item.get("range")}],
                       "steps": ["describe/read 当前设置取得值、可写性、Schema 与 valueVersion", "用户要求修改时按 Schema 设置；联动差异纳入 preview，比较版本提交", "回读并保存 changeId；不改磁盘全局配置绕过"],
                       "branches": ["writable=false 说明具体限制，不代表整个功能不存在", "敏感值不回显，不提交遮蔽占位值"],
                       "verification": "verified 及回读值；回退比较当前版本", "references": [ref("settings.md")],
                       "source": {"path": item["source"], "line": item["line"]}, "sourceRevision": revision})
    for item in data["views"]:
        owner = item["class"] or item["source"]
        result.append({"id": "view." + owner, "kind": "page", "title": owner.split(".")[-1] if item["class"] else Path(owner).stem,
                       "summary": "页面／窗口及绑定入口", "keywords": [item["source"]], "availability": "主页面走 open_page，编辑窗口核对具体命令",
                       "steps": ["list_pages 返回主页面标识；主页面用 open_page", "不是主页面时搜索具体打开命令；不调用 Loaded/Closing 等内部事件"],
                       "inputSources": [], "bindings": {"commands": [binding for binding in item["commands"] if any(name in binding["binding"] for name in published_names)]}, "branches": [], "verification": "导航选择与窗口可见性；打开页面不代替业务操作",
                       "references": [ref("viewmodel-usage.md", "bgi-assistant")], "source": {"path": item["source"]}, "sourceRevision": revision})
    for kind, records in [("scriptApi", data["scriptApis"]), ("resourceModel", data["resourceModels"])]:
        for item in records:
            if item.get("exposureReason"):
                continue
            result.append({"id": item["symbol"], "kind": kind, "title": item["summary"] or item["name"],
                           "summary": item["summary"], "keywords": [item["name"], item["owner"]], "availability": "脚本 API／资源模型，不是直接桥操作",
                           "inputSources": item.get("parameters") or [{"type": item["valueType"]}],
                           "declaration": {"owner": item["owner"], "name": item["name"], "valueType": item["valueType"], "isStatic": item["isStatic"], "writable": item["writable"], "jsonName": item.get("jsonName"), "sourceDefault": item.get("initial")},
                           "steps": ["从具体脚本／资源的引用定位，不猜宿主 API 或对象字段", "读取相关源码范围与调用方；参数从实际定义取得"],
                           "branches": ["源码存在不代表 Agent 已有同名工具"], "verification": "真实字段引用、分支与调用顺序证据",
                           "references": [{"skill": "bgi-javascript", "tool": "skills.read"}] if kind == "scriptApi" else [ref("resources.md")],
                           "source": {"path": item["source"], "line": item["line"]}, "sourceRevision": revision})
    # 稳定入口的卡片链接到更有业务含义的流程。
    for item in data["stableEntries"]:
        method = item["methodId"]
        linked = [flow["id"] for flow in WORKFLOWS if method in flow["methodIds"]]
        result.append({"id": method, "kind": "stable", "title": method, "summary": "稳定宿主入口，完整目标流程见 related", "keywords": [method],
                       "availability": "必须读取当前契约", "methodIds": [method], "related": linked,
                       "steps": ["describe 当前稳定入口，按 Schema 提供参数", "只读接口 read、写接口 invoke，按当前权限提交并核验"],
                       "branches": ["接口未登记／不可调用时说明版本或具体前置，不反射扫描绕过"],
                       "verification": "当前契约要求的结构化证据", "references": [ref("shared.md")], "inputSources": [], "source": {"path": item["source"]}})
    ids = [item["id"] for item in result]
    assert len(ids) == len(set(ids)), "功能 ID 重复"
    return {"format": 1, "sourceRevision": revision, "items": result}


def main():
    data = json.loads(AUDIT.read_text(encoding="utf-8"))
    index = build(data)
    encoded = json.dumps(index, ensure_ascii=False, separators=(",", ":"))
    if "--check" in sys.argv:
        if not OUTPUT.exists() or OUTPUT.read_text(encoding="utf-8") != encoded:
            raise SystemExit("功能索引已过期；运行 feature-guide-builder.py 后再验证")
    else:
        OUTPUT.parent.mkdir(parents=True, exist_ok=True)
        OUTPUT.write_text(encoded, encoding="utf-8")
    print(json.dumps({"items": len(index["items"]), "workflows": len(WORKFLOWS), "bytes": OUTPUT.stat().st_size}))


if __name__ == "__main__":
    main()
