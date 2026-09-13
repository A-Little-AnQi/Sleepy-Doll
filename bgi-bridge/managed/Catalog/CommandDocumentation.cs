using System.Text.RegularExpressions;

namespace BgiBridge.Catalog;

/// <summary>Domain-aware explanations supplement source comments and localized UI captions.</summary>
public static class CommandDocumentation
{
    private static readonly Dictionary<string, string> Titles = new(StringComparer.Ordinal)
    {
        ["MainWindow.Activated"] = "主窗口激活时检查剪贴板",
        ["MainWindow.Closing"] = "处理主窗口关闭请求",
        ["MainWindow.Loaded"] = "初始化 BetterGI 主窗口",
        ["MainWindow.DismissRedeemCode"] = "关闭兑换码更新提示",
        ["MainWindow.OpenFeed"] = "打开兑换码动态窗口",
        ["MaskWindow.Loaded"] = "初始化游戏遮罩",
        ["MaskWindow.OverlayLayoutCommitted"] = "保存遮罩控件布局",
        ["MaskWindow.WindowSizeChanged"] = "同步遮罩窗口尺寸",
        ["MaskWindow.PointClick"] = "打开地图点位详情",
        ["MaskWindow.PointRightClick"] = "切换地图点位隐藏状态",
        ["MaskWindow.PointHover"] = "处理地图点位悬停",
        ["HomePage.Loaded"] = "初始化 BetterGI 启动页",
        ["HomePage.ManualPickWindow"] = "选择游戏窗口并启动截图器",
        ["MusicPage.SelectMusicFolder"] = "切换曲谱目录",
        ["KeyMouseRecordPage.StopRecord"] = "停止并保存键鼠录制",
        ["CommonSettingsPage.GameLangSelectionChanged"] = "重载游戏语言 OCR 模型",
        ["ScriptGroupConfig.AutoFightEnabledChecked"] = "同时启用路线追踪配置",
        ["CommonSettingsPage.QuestionButtonOnClick"] = "打开日志分析窗口",
        ["OneDragonFlow.ConfigDropDownChanged"] = "同步一条龙配置选择",
        ["TaskSettingsPage.RunInventoryCountComparison"] = "运行背包数量 OCR 对比",
        ["NotificationSettingsPage.TestWebhook"] = "发送 Webhook 测试通知",
        ["NotificationSettingsPage.BindQq"] = "绑定 QQ 用户通知目标",
        ["NotificationSettingsPage.CancelBindQq"] = "取消 QQ 用户绑定",
        ["NotificationSettingsPage.BindGroupQq"] = "绑定 QQ 群通知目标",
        ["NotificationSettingsPage.CancelBindGroupQq"] = "取消 QQ 群绑定",
        ["NotificationSettingsPage.BindWechatClawbot"] = "绑定微信 Clawbot 通知目标",
        ["NotificationSettingsPage.CancelBindWechatClawbot"] = "取消微信 Clawbot 绑定",
    };
    private static readonly HashSet<string> InternalUiEvents = new(StringComparer.Ordinal)
    {
        "Activated", "Closing", "Loaded", "WindowSizeChanged", "OverlayLayoutCommitted",
        "PointClick", "PointHover", "PointRightClick", "DropDownChanged", "ConfigDropDownChanged",
        "CaptureModeDropDownChanged",
    };
    private static readonly Dictionary<string, string> Terms = new(StringComparer.Ordinal)
    {
        ["AutoGeniusInvokation"]="七圣召唤", ["AutoLeyLineOutcrop"]="地脉花",
        ["AutoStygianOnslaught"]="幽境危战", ["ArtifactSalvage"]="圣遗物分解",
        ["AutoRedeemCode"]="兑换码", ["AutoMusicGame"]="音游", ["AutoAlbum"]="千音雅集",
        ["AutoFishing"]="自动钓鱼", ["AutoDomain"]="自动秘境", ["AutoFight"]="自动战斗",
        ["AutoBoss"]="首领讨伐", ["AutoCook"]="自动烹饪", ["AutoWood"]="自动伐木",
        ["AutoTrackPath"]="地图追踪", ["AutoTrack"]="旧版自动寻路",
        ["GetGridIcons"]="背包图标采集", ["GridIconsModelAccuracyTest"]="背包图标模型准确率测试",
        ["AutoEat"]="自动吃药", ["OneDragonFlow"]="一条龙流程", ["ScriptControl"]="脚本调度",
        ["JsList"]="JavaScript 脚本", ["KeyMouseRecordPage"]="键鼠脚本",
        ["MapPathing"]="地图追踪路线", ["MusicPage"]="音乐播放", ["FeedWindow"]="动态订阅",
        ["CommonSettingsPage"]="通用设置", ["TaskSettingsPage"]="独立任务设置",
        ["TriggerSettingsPage"]="触发器设置", ["NotificationSettingsPage"]="通知渠道设置",
        ["MacroSettingsPage"]="宏设置", ["HardwareAcceleration"]="硬件加速",
        ["PathingConfig"]="地图追踪条件配置", ["ScriptGroupConfig"]="脚本组配置",
        ["ChildSessionWindow"]="桌面分身", ["CustomHtmlMaskEditor"]="HTML 遮罩编辑器",
        ["RecognitionTemplateEditor"]="识别模板编辑器", ["MainWindow"]="主窗口",
        ["MaskWindow"]="游戏遮罩", ["MaskMapPointInfoPopup"]="地图点位信息",
        ["AutoPickBlacklistConfig"]="拾取黑名单", ["AutoPickWhitelistConfig"]="拾取白名单",
        ["HomePage"]="启动页", ["NotifyIcon"]="托盘菜单", ["Form"]="配置表单",
        ["JsonMono"]="JSON 编辑器", ["WebImageInput"]="网络图片输入",
        ["MapPathingDev"]="地图追踪开发面板", ["Drawer"]="详情抽屉",
    };
    private static readonly Dictionary<string, string> Actions = new(StringComparer.Ordinal)
    {
        ["StartTrigger"]="启动截图器、识别调度和已启用触发器；要求已选定游戏窗口。",
        ["StopTrigger"]="停止截图调度并请求取消独立任务，同时隐藏相关遮罩。",
        ["StopSoloTask"]="向宿主独立任务发送取消请求并重置任务开关；需要继续观测任务是否已停止。",
        ["SOneDragonFlow"]="执行当前选中的一条龙配置；未选中配置时不会启动。",
        ["OneKeyExecute"]="按当前一条龙配置依次执行任务；需先核对任务列表和资源。",
        ["StartMultiScriptGroup"]="按界面选中列表启动多个脚本组。",
        ["ContinueMultiScriptGroup"]="从界面记录的断点继续执行多个脚本组。",
        ["StartScriptGroup"]="启动指定或当前选中的脚本组，执行组内配置的任务。",
        ["StartRun"]="运行所选 JavaScript 脚本，可能产生脚本定义的游戏和文件副作用。",
        ["StartPlay"]="回放所选键鼠录制脚本，将向目标窗口发送输入。",
        ["StartRecord"]="开始录制键盘和鼠标操作，结束后需要保存录制结果。",
        ["StopRecord"]="停止当前键鼠录制，把已录制的键鼠事件序列化为 JSON，并在宿主界面打开保存窗口；未处于录制状态时拒绝执行。",
        ["ManualPickWindow"]="打开目标窗口选择器；用户选定窗口后立即将它设为捕获目标并启动 BetterGI 截图器。用户取消选择时不修改当前目标。",
        ["RefreshMaskSettings"]="通知遮罩重新读取设置并计算控件位置。",
        ["ResetMaskOverlayLayout"]="恢复遮罩布局默认值，会修改相关配置。",
        ["ResetOverlayStyle"]="恢复遮罩样式默认值并刷新显示，会修改相关配置。",
        ["SwitchBackdrop"]="切换主窗口背景材质或背景显示模式。",
        ["Hide"]="隐藏主窗口，不等于退出或停止后台任务。",
        ["ShowOrHide"]="切换主窗口显示状态，不停止后台任务。",
        ["Exit"]="退出 BetterGI，桥随宿主退出；不能把连接断开当成可重试错误。",
        ["CheckUpdate"]="检查 BetterGI 更新信息，可能访问网络并打开更新界面。",
        ["OpenFeed"]="打开动态订阅窗口。",
        ["DismissRedeemCode"]="只关闭主窗口中的兑换码更新提示卡片；不打开动态窗口、不读取兑换码，也不执行兑换。",
        ["ToggleHidden"]="切换当前地图点位的隐藏状态。",
        ["ToggleMapPointHidden"]="切换指定地图点位的隐藏状态。",
        ["ToggleMapPointPicker"]="切换地图点位拾取或选择模式。",
        ["SelectMapLabelCategory"]="选择地图标签分类并更新可选项。",
        ["SelectMapLabelItem"]="选择地图标签条目并更新点位显示。",
        ["ResetSelectedMapLabelSelection"]="清空当前地图标签筛选选择。",
        ["ShowAllMapPoints"]="显示当前地图范围的全部已加载点位。",
        ["HideAllMapPoints"]="隐藏当前地图点位显示。",
        ["ExitOverlayLayoutEditMode"]="退出遮罩布局编辑模式。",
        ["SetRightClickSelection"]="将右键指向的资源设为当前选择，供后续菜单操作使用。",
        ["StrategyDropDownOpened"]="刷新对应任务的策略候选列表，不启动任务。",
        ["CaptureModeDropDownChanged"]="应用截图模式选择；截图器运行时可能先停止再重新启动。",
        ["GameLangSelectionChanged"]="卸载当前 OCR 实例，使下一次识别按已经保存的游戏语言配置重新创建模型；本命令自身不修改语言设置。",
        ["UiLanguageSelectionChanged"]="应用软件界面语言选择。",
        ["PaddleOcrModelConfigChanged"]="应用 Paddle OCR 模型选择并更新识别配置。",
        ["AutoPickModeChanged"]="应用自动拾取的黑名单或白名单模式选择。",
        ["AutoFightEnabledChecked"]="勾选脚本组自动战斗后，同时把该脚本组的路线追踪配置 Enabled 设为 true；会修改配置。",
        ["GetExecutionOrder"]="根据当前脚本组配置计算或显示任务执行顺序。",
        ["CopyTask"]="复制选中的流程任务条目。",
        ["DeleteTask"]="从当前流程配置移除所选任务条目。",
        ["SetTaskAsNext"]="将所选任务设为后续连续执行的起点。",
        ["SaveActionConfig"]="保存当前流程动作的设置。",
        ["DeleteConfigDisplayTaskListFromConfig"]="移除当前配置中的任务显示列表条目。",
        ["BeginSeek"]="进入音乐播放进度拖动状态。",
        ["Seek"]="把音乐播放位置跳转到请求的进度。",
        ["CyclePlaybackMode"]="轮换音乐顺序播放、循环等播放模式。",
        ["PlaySelected"]="播放当前选中的音乐轨道。",
        ["UpdateTrackSelection"]="同步音乐轨道选择。",
        ["TestWebhook"]="向配置的 Webhook 接收方发送测试通知，并更新宿主测试状态；可能包含截图，需用户授权接收方。",
        ["QuestionButtonOnClick"]="打开内置日志分析网页窗口，显示由宿主生成的统计内容；不修改游戏状态。",
        ["ConfigDropDownChanged"]="把一条龙页面切换到当前 SelectedConfig，并清空已选任务；依赖宿主下拉框已经设置选择。",
        ["RunInventoryCountComparison"]="按当前选择的对比目标启动背包数量 OCR 对比独立任务；运行期间临时打开背包图标采集状态，结束后关闭。",
        ["BindQq"]="连接 QQ 网关并等待用户发送验证码，成功后自动回填 QQ 用户 OpenID；要求已配置 AppID 与 AppSecret，可等待最多 60 秒。",
        ["CancelBindQq"]="取消正在等待的 QQ 用户绑定 WebSocket 流程；没有绑定流程时不执行其他操作。",
        ["BindGroupQq"]="连接 QQ 网关并等待机器人加入群聊或收到验证码，成功后自动回填群 OpenID；要求已配置 AppID 与 AppSecret。",
        ["CancelBindGroupQq"]="取消正在等待的 QQ 群绑定 WebSocket 流程；不删除已经保存的群 OpenID。",
        ["BindWechatClawbot"]="启动微信 Clawbot 扫码登录与一次性验证码绑定；成功后一次性保存 bot token、用户 ID 和上下文 token，失败或取消时不应留下混合凭据。",
        ["CancelBindWechatClawbot"]="取消正在进行的微信 Clawbot 登录或验证码绑定；不主动删除已经保存的凭据。",
        ["RefreshMapping"]="重新加载音乐按键映射。",
        ["SelectMusicFolder"]="使用 argument 指定的已有目录切换曲谱库：停止当前播放、保存目录历史与配置、切换目录监听并刷新曲目列表。它不打开目录选择器。",
        ["ToggleAudioMuted"]="切换桌面分身的静音状态。",
        ["ToggleGameMouseMode"]="切换桌面分身的游戏鼠标交互模式。",
        ["ToggleSmallWindowMode"]="切换桌面分身的小窗口显示模式。",
        ["ToggleTopmost"]="切换桌面分身窗口置顶。",
        ["RestoreDefaultCustomHtmlMask"]="恢复默认 HTML 遮罩内容，可能覆盖自定义文件。",
        ["SaveCustomHtmlMask"]="保存 HTML 遮罩编辑内容到文件。",
        ["ToggleCustomHtmlMaskPreview"]="切换 HTML 遮罩预览。",
        ["NormalizeTemplateFileName"]="按识别模板规则规范化文件名。",
        ["SubmitWebImageUrl"]="提交图片 URL 供宿主加载或校验；可能访问网络。",
    };

    public static string Title(string viewModel, string command, SourceEntry? source)
    {
        var owner = viewModel.Replace("ViewModel", "");
        var name = Normalize(command);
        if (Titles.TryGetValue(owner + "." + name, out var title)) return title;
        var scope = Terms.GetValueOrDefault(owner, owner);
        var label = source?.Label?.Trim();
        if (!string.IsNullOrWhiteSpace(label) && label != "关闭" && label != "打开" && label != "删除")
            return label.Contains(scope, StringComparison.OrdinalIgnoreCase) ? label : $"{scope}：{label}";
        var purpose = Purpose(viewModel, command, source);
        var sentence = purpose.Split('。', '；')[0].Trim();
        return sentence.Length <= 36 ? sentence : $"{scope}：{Humanize(name)}";
    }

    public static string? UnavailableReason(string viewModel, string command)
    {
        var name = Normalize(command);
        if (!InternalUiEvents.Contains(name)) return null;
        var owner = viewModel.Replace("ViewModel", "");
        return $"{Terms.GetValueOrDefault(owner, owner)}的 {name} 是宿主控件生命周期/输入事件，参数和调用顺序由 WPF 维护；目录保留真实说明供审计，但 Agent 不得脱离界面事件模拟调用。";
    }

    public static bool RequiresGameReady(string viewModel, string command)
    {
        var owner = viewModel.Replace("ViewModel", "");
        var name = Normalize(command);
        if (name.StartsWith("SwitchAuto", StringComparison.Ordinal)) return true;
        return (owner, name) switch
        {
            ("JsList", "StartRun") or
            ("KeyMouseRecordPage", "StartPlay") or
            ("MapPathing", "Start") or
            ("MusicPage", "PlaySelected") or
            ("OneDragonFlow", "OneKeyExecute") or
            ("ScriptControl", "StartMultiScriptGroup") or
            ("ScriptControl", "ContinueMultiScriptGroup") or
            ("ScriptControl", "StartScriptGroup") or
            ("TaskSettingsPage", "SOneDragonFlow") or
            ("TaskSettingsPage", "RunInventoryCountComparison") => true,
            _ => false,
        };
    }

    public static string[] SideEffects(string viewModel, string command)
    {
        var name = Normalize(command);
        if (RequiresGameReady(viewModel, command))
            return ["可能启动任务或向游戏窗口发送输入；范围由用途说明限定。"];
        if (name.Contains("Notification", StringComparison.Ordinal)
            || name is "TestWebhook" or "SubmitWebImageUrl" or "CheckUpdate")
            return ["可能访问网络或向已配置接收方发送数据。"];
        if (new[] { "Delete", "Remove", "Clear", "Reset", "Restore", "Save", "Import", "Export", "Rename" }
            .Any(verb => name.StartsWith(verb, StringComparison.Ordinal)))
            return ["可能修改用途说明所指的配置、集合或文件。"];
        if (new[] { "Open", "GoTo", "Show", "Hide", "Close" }
            .Any(verb => name.StartsWith(verb, StringComparison.Ordinal)))
            return ["改变 BetterGI 界面或打开本地/网页目标。"];
        return ["改变该 ViewModel 的当前选择或界面状态；具体变化以用途说明为准。"];
    }

    public static string Verification(string viewModel, string command)
    {
        var name = Normalize(command);
        if (RequiresGameReady(viewModel, command))
            return "查询 Job 终态，再读取运行状态或任务产物；executed=true 不是游戏目标完成。";
        if (new[] { "Delete", "Remove", "Clear", "Reset", "Restore", "Save", "Import", "Export", "Rename" }
            .Any(verb => name.StartsWith(verb, StringComparison.Ordinal)))
            return "重新读取用途说明所指的配置、集合或文件，确认目标变化。";
        return "核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。";
    }

    public static string Rollback(string viewModel, string command)
    {
        var name = Normalize(command);
        if (RequiresGameReady(viewModel, command))
            return "游戏输入和任务进度不能由配置检查点撤销；需要时使用对应停止命令并核验终态。";
        if (new[] { "Save", "Delete", "Remove", "Clear", "Import", "Export", "Rename" }
            .Any(verb => name.StartsWith(verb, StringComparison.Ordinal)))
            return "配置检查点不覆盖脚本及其他资源文件；只能按目标资源自己的备份或重新写入恢复。";
        return "仅配置字段可使用 configurationCheckpoint 离线恢复；普通界面状态通常无需回退。";
    }

    public static string Purpose(string viewModel, string command, SourceEntry? source)
    {
        if (source?.HasImplementation == false) return "此命令在当前宿主源码中为空实现，不执行任何业务操作；仅保留目录记录，不应安排调用。";
        var name = Normalize(command);
        var scope = Terms.GetValueOrDefault(viewModel.Replace("ViewModel", ""), viewModel.Replace("ViewModel", ""));
        var owner = viewModel.Replace("ViewModel", "");
        if (owner == "MainWindow" && name == "Activated")
            return "BetterGI 主窗口再次获得焦点时读取不超过 1000 字符的剪贴板文本，并依次尝试导入脚本和识别兑换码；首次激活跳过。此流程读取用户剪贴板，识别成功时可能写入脚本或兑换码数据。";
        if (owner == "MainWindow" && name == "Closing")
            return "处理主窗口的关闭事件：桌面分身仍运行时取消关闭并提示；启用“关闭到托盘”时取消退出并隐藏主窗口。它不等同于强制退出 BetterGI。";
        if (owner == "MainWindow" && name == "Loaded")
            return "执行主窗口的一次性启动初始化，包括应用主题、版本迁移和目录权限调整、OCR 预热、命令行任务处理及更新检查；重复调用可能重复启动初始化流程。";
        if (owner == "MaskWindow" && name == "Loaded")
            return "遮罩窗口加载后刷新遮罩设置、初始化任务状态列表与性能指标显示；这是窗口生命周期初始化，不是启动截图器。";
        if (owner == "MaskWindow" && name == "OverlayLayoutCommitted")
            return "把用户拖动后的日志、任务状态或性能指标控件位置与尺寸换算成窗口比例并写回遮罩配置。参数只能由遮罩布局编辑器产生。";
        if (owner == "MaskWindow" && name == "WindowSizeChanged")
            return "同步新的遮罩窗口宽高，并重新计算相对 1080p 的缩放比例，使日志字号随游戏分辨率变化。参数来自 WPF SizeChangedEventArgs。";
        if (owner == "MaskWindow" && name == "PointClick")
            return "在游戏大地图界面点击遮罩点位时打开该点位详情弹窗；参数包含宿主地图点位对象和锚点位置。";
        if (owner == "MaskWindow" && name == "PointRightClick")
            return "右键点击遮罩地图点位时切换该点位的隐藏状态；参数是宿主加载的完整地图点位对象。";
        if (owner == "MaskWindow" && name == "PointHover")
            return "接收遮罩地图点位悬停事件；当前宿主实现不执行实际业务操作。";
        if (name == "Hide") return $"隐藏{scope}窗口，不等于退出或停止后台任务。";
        if (Actions.TryGetValue(name, out var action)) return action;
        if (name.StartsWith("Switch") && Terms.TryGetValue(name[6..], out var task))
            return $"使用宿主当前配置启动{task}任务；需要游戏和截图器就绪、独立任务空闲。命令返回不等于目标已验证完成。";
        if (name.StartsWith("GoTo") && name.EndsWith("Url"))
            return $"在浏览器打开{Terms.GetValueOrDefault(name[4..^3], name[4..^3])}的使用文档，不执行游戏操作。";
        if (name.StartsWith("Test") && name.EndsWith("Notification"))
            return $"向已配置的 {name[4..^12]} 通知渠道发送测试消息，会产生外部通信；需用户授权该接收方。";
        if (!string.IsNullOrWhiteSpace(source?.Summary) && source.Summary.Length <= 220
            && !new[] { "[RelayCommand]", "private ", "public ", "=>", "{" }.Any(token => source.Summary.Contains(token, StringComparison.Ordinal)))
            return $"{scope}：{source.Summary}";
        if (name is "Loaded" or "Activated" or "Closing" or "WindowSizeChanged" or "OverlayLayoutCommitted"
            or "PointClick" or "PointHover" or "PointRightClick" or "DropDownChanged" or "ConfigDropDownChanged")
            return $"{scope}的 {name} 由宿主界面事件触发；当前源码未提供足以支持脱离控件上下文调用的业务契约，因此只保留接口记录，不安排 Agent 调用。";
        if (name is "Initialize") return $"初始化{scope}的数据和界面状态；不应在已运行的页面中反复触发。";
        if (name == "Refresh") return $"重新加载{scope}的数据来源并更新界面列表；完成后应重新读取列表确认变化。";
        if (name is "Save" or "EditAt" or "RemoveAt") return name switch
        {
            "Save" => $"将{scope}当前编辑内容保存到宿主管理的数据位置；可能覆盖原文件。",
            "EditAt" => $"编辑{scope}中指定位置的条目，依赖当前表单数据。",
            _ => $"删除{scope}中指定位置的条目，依赖当前表单数据。",
        };
        var verbs = new Dictionary<string,string> { ["Open"]="打开",["GoTo"]="转到",["Add"]="添加",["Copy"]="复制",["Delete"]="删除",["Remove"]="移除",["Rename"]="重命名",["Edit"]="编辑",["Import"]="导入",["Export"]="导出",["Close"]="关闭",["Toggle"]="切换",["Reset"]="重置",["Start"]="启动",["Stop"]="停止",["Update"]="更新",["Clear"]="清空" };
        foreach (var (verb, translated) in verbs)
            if (name.StartsWith(verb))
            {
                var target = name[verb.Length..];
                var nouns = new Dictionary<string,string> { ["ScriptGroup"]="脚本组",["JsScript"]="JavaScript 脚本",["KmScript"]="键鼠脚本",["Pathing"]="地图追踪路线",["Shell"]="Shell 动作",["Script"]="脚本",["Config"]="配置",["ScriptsFolder"]="脚本目录",["ScriptFolder"]="脚本目录",["ScriptProjectFolder"]="脚本项目目录",["LocalScriptRepo"]="本地脚本仓库",["Settings"]="设置窗口",["ChildSessionWindow"]="桌面分身窗口",["FightFolder"]="战斗策略目录",["HotKeyPage"]="快捷键设置页",["LogFolder"]="日志目录",["CustomHtmlMaskFolder"]="HTML 遮罩目录",["CustomHtmlMaskEditor"]="HTML 遮罩编辑器",["ScriptGroupSettings"]="脚本组设置",["PathingDetail"]="路线详情",["ScriptDetailDrawer"]="脚本详情抽屉",["DevTools"]="开发者工具",["CacheFolder"]="缓存目录",["AvatarConditionConfig"]="角色条件",["PartyConditionConfig"]="队伍条件",["JsScriptSettings"]="JavaScript 脚本设置",["ScriptCommon"]="脚本通用设置",["AvatarMacro"]="角色宏",["SkillCdConfig"]="技能冷却显示配置",["BlacklistModeConfig"]="拾取黑名单配置",["WhitelistModeConfig"]="拾取白名单配置",["Drawer"]="详情抽屉" };
                var targetLabel = nouns.GetValueOrDefault(target, Regex.Replace(target, "([a-z])([A-Z])", "$1 $2"));
                return $"在{scope}中{translated}{targetLabel}。依赖该页面当前选择；如出现文件或输入对话框，需要用户在宿主完成交互。";
            }
        return $"{scope}中的 {Humanize(name)} 命令。当前宿主没有提供足以确定目标、副作用和结果的业务说明；目录保留该真实命令供审计，不安排自动调用。";
    }

    private static string Normalize(string command)
    {
        var name = command.EndsWith("Command") ? command[..^7] : command;
        if (name.StartsWith("On") && name.Length > 2 && char.IsUpper(name[2])) name = name[2..];
        return name;
    }

    private static string Humanize(string name) => Regex.Replace(name, "([a-z])([A-Z])", "$1 $2");
}
