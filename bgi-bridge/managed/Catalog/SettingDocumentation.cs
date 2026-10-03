using System.Reflection;

namespace BgiBridge.Catalog;

/// <summary>结合宿主配置消费方与界面绑定核对过的业务说明。</summary>
public static class SettingDocumentation
{
    private static readonly Dictionary<string, string> Reviewed = new(StringComparer.Ordinal)
    {
        ["AutoBossConfig.BossName"] = "自动讨伐的首领名称；从 BetterGI 支持的首领列表选择，修改名称不会立即开始讨伐。",
        ["CommonConfig.ScreenshotEnabled"] = "启用快捷键保存截图功能，截图保存到 log/screenshot；源码注释写作“是否启用遮罩窗口”已过时，遮罩窗口的显示设置在 maskWindowConfig 分区，不由此项控制。",
        ["AutoDomainConfig.WalkToF"] = "步行前往开启秘境和领取奖励：开启后前往交互点时不按冲刺键，关闭则按住冲刺键疾跑；源码注释与 shortMovement 的注释重复，属于复制错误。",
        ["AutoSkipConfig.ClickChatOption"] = "剧情对话选项点击策略，合法取值：优先选择第一个选项 / 随机选择选项 / 优先选择最后一个选项 / 不选择选项；源码注释缺少“随机选择选项”一档。",
        ["AutoGeniusInvokationConfig.DefaultCharacterCardRects"] = "七圣召唤无法正常识别角色卡区域时使用的默认矩形列表；属于图像识别坐标配置，不是卡牌名称。复杂 Rect 集合只读。",
        ["HotKeyConfig.AutoTrackHotkey"] = "旧自动寻路功能保留的快捷键字段；当前源码中的注册入口已注释，不应声称设置后即可触发寻路。",
        ["HotKeyConfig.AutoTrackHotkeyType"] = "旧自动寻路快捷键的监听方式；当前源码未注册此快捷键，保留字段不代表功能可用。",
        ["HotKeyConfig.AutoTrackPathHotkeyType"] = "旧地图追踪快捷键的监听方式；当前源码的对应注册入口已注释。",
        ["HotKeyConfig.MapPosRecordHotkeyType"] = "旧地图坐标记录快捷键的监听方式；当前源码的对应注册入口已注释。",
        ["AutoFightConfig.GuardianAvatar"] = "盾奶位角色的队伍位置，使用字符串 1、2、3 或 4；空字符串关闭实时盾奶位技能检测。不是角色名称。",
        ["AutoFightConfig.GuardianCombatSkip"] = "跳过盾奶位角色的常规战斗策略，改由实时技能检测处理；需要先设置 guardianAvatar。",
        ["AutoFightConfig.GuardianAvatarHold"] = "盾奶位角色的元素战技按法：true 长按，false 短按；需要先设置 guardianAvatar。",
        ["AutoFightConfig.BurstEnabled"] = "允许盾奶位实时检测释放元素爆发 Q：元素战技 E 就绪时优先放 E，仅 E 未就绪时才检测并释放 Q。开启后盾奶位与 guardianCombatSkip 一样完全退出常规策略执行；需要先设置 guardianAvatar。",
        ["AutoFightConfig.QinDoublePickUp"] = "战后允许二次拾取：琴首次未拾取到时再次尝试，万叶即使刚释放过技能也强制再拾取；依赖战后拾取配置。",
        ["AutoFightConfig.SwimmingEnabled"] = "战斗中启用游泳检测；确认角色落水且有战斗点位时先尝试返回战斗地点，失败则传送七天神像恢复后重试。不是主动游泳指令。",
        ["AutoFightConfig.StrategyName"] = "自动战斗使用的策略名称；按 BetterGI 当前可用策略选择，不编造文件名。配置本身不启动战斗。",
        ["AutoFightConfig.Timeout"] = "单场战斗超时，单位秒，默认 120；达到上限强制结束本场战斗，且超时结束会跳过换队拾取。旋转找敌累计 6 次未发现敌人时同样强制结束。",
        ["AutoFightConfig.TeamNames"] = "强制指定队伍角色，中英文逗号均可分隔；必须恰好 4 个角色名，否则启动战斗时报错。填写后跳过队伍画面识别，别名会转换为标准名并回写此字段。",
        ["AutoFightConfig.OnlyPickEliteDropsMode"] = "只拾取精英掉落的模式，仅在地图追踪点位开启按怪物标签拆分拾取、且标签为普通怪时生效。AllowAutoPickupForNonElite 仅关闭该场的万叶与光柱拾取；DisableAutoPickupForNonElite 还会暂停全局自动拾取直到本场战斗结束。其余场景此配置无效果。",
        ["AutoFightConfig.PickDropsAfterFightEnabled"] = "战斗结束后扫描拾取掉落物，在万叶/琴拾取之后执行，与队伍中是否存在万叶或琴无关；开启后即使视野内没有物品也会持续扫描满 pickDropsAfterFightSeconds 秒（默认 15 秒）。设置界面“仅在无万叶和琴时备用”的说明与当前代码不符。",
        ["AutoFightConfig.KazuhaPickupEnabled"] = "战后聚集拾取总开关：队伍有枫原万叶用万叶长E，没有万叶但有琴时改用琴长E；二次拾取与基于经验值的拾取判断都要求此项开启。",
        ["AutoFightConfig.KazuhaPartyName"] = "战后当前队伍没有万叶和琴、且战斗非超时结束时，切换到该名称的队伍执行拾取再切回原队伍；当前队伍名未知时会打开编队界面 OCR 读取，名称必须与编队界面显示一致。",
        ["AutoFightConfig.DamageNumberRecognitionMode"] = "伤害数字识别模式：0 禁用、1 OCR 识别、2 颜色识别（默认颜色）。用于持续索敌和旋转寻敌在血条不可见时以伤害数字或元素反应文字定位敌人。",
        ["FightFinishDetectConfig.PaimonEndCheckEnabled"] = "派蒙辅助检测：按下打开编队界面的键（默认 L）后，派蒙头像仍可见说明编队界面未打开、战斗未结束，按取消键提前跳出本次结束检查；头像消失才继续色块判定。看到派蒙头像等于战斗仍在进行，不是已结束。默认关闭。",
        ["FightFinishDetectConfig.PaimonEndCheckDelay"] = "按下编队键后到检测派蒙头像的等待，单位秒，运行时钳制在 0.05–0.4；代码初始值 0.2，设置页“默认为0.1秒”的文案与代码不一致。",
        ["FightFinishDetectConfig.CheckEndDelay"] = "触发战斗结束检查后先等待的时长，单位秒，用于等动作后摇；空值或无法解析时按 1500 毫秒处理。可按角色覆盖，格式如 0.4 或 0.4;钟离,1.5。",
        ["FightFinishDetectConfig.BeforeDetectDelay"] = "按下编队键打开界面后到检查屏幕色块之间的等待，单位秒；空值或无法解析时按 0.45 处理，新配置默认 0.4。无法结束战斗时可适当提高（如 0.75），过大会把队伍配置界面真正切出来。",
        ["FightFinishDetectConfig.CheckBeforeBurst"] = "释放元素爆发 Q 前先做一次战斗结束检查；仅在开启旋转寻找敌人位置后生效。",
        ["FightFinishDetectConfig.IsFirstCheck"] = "尝试面敌：开战首轮先寻敌并面向敌人再执行策略；仅在开启旋转寻找敌人位置后生效。",
        ["FightFinishDetectConfig.RotaryFactor"] = "旋转寻找敌人的速度因子，运行时钳制在 1–13，越大旋转越快；默认 12。",
        ["AutoLeyLineOutcropFightConfig.SeekEnemyEnabled"] = "地脉战斗是否定时旋转搜索敌人；与寻敌间隔和旋转因子一起生效。",
        ["AutoLeyLineOutcropFightConfig.SeekEnemyIntervalSeconds"] = "地脉战斗两次旋转寻敌之间的间隔，单位秒。",
        ["AutoLeyLineOutcropFightConfig.SeekEnemyRotaryFactor"] = "地脉战斗旋转寻敌的速度因子；不是角度或持续秒数。",
        ["AutoLeyLineOutcropConfig.Timeout"] = "自动地脉花的战斗超时，单位秒；达到上限会结束相应战斗等待，不代表已领取奖励。",
        ["AutoLeyLineOutcropConfig.IsGoToSynthesizer"] = "地脉花流程是否前往合成台处理浓缩树脂；会改变后续流程，修改本项本身不移动角色。",
        ["WindowPositionConfig.Left"] = "桌面分身窗口左边缘的屏幕物理像素坐标；所在路径区分普通窗口或小窗模式，多屏环境可为负数。",
        ["WindowPositionConfig.Top"] = "桌面分身窗口上边缘的屏幕物理像素坐标；所在路径区分普通窗口或小窗模式，多屏环境可为负数。",
        ["MapMaskConfig.HoYoLabLanguage"] = "HoYoLAB 地图点位数据的语言；仅在选用 HoYoLAB 数据源时影响请求与缓存，不是游戏或软件界面语言。",
        ["MapMaskConfig.MapPointApiProvider"] = "地图遮罩点位的数据提供方，决定从哪个地图服务加载标签、点位及详情；可能引发网络请求和缓存切换。",
        ["MaskWindowConfig.CustomHtmlMaskEnabled"] = "启用自定义 HTML 遮罩内容；影响游戏上的遮罩显示，不等于启停游戏截图器。",
        ["MaskWindowConfig.LogFontFamily"] = "日志遮罩使用的字体族名称；需使用本机可用字体，不修改日志内容。",
        ["MaskWindowConfig.MetricsFontFamily"] = "性能指标遮罩使用的字体族名称；需使用本机可用字体，不改变指标采样。",
        ["NotificationConfig.NotificationEventSubscribe"] = "通知事件订阅列表，用于筛选哪些 BetterGI 事件触发通知；渠道与接收方另由各通知渠道配置决定。",
        ["AutoRestart.Enabled"] = "启用调度器任务连续异常后的自动重启机制；阈值取 failureCount，不会在修改开关时立即重启。",
        ["FarmingPlan.Enabled"] = "启用锄地规划中的每日击杀上限控制；配合每日精英、小怪上限及可选米游社数据使用。",
        ["MiyousheDataSupport.Enabled"] = "锄地规划是否使用米游社数据辅助计算当日击杀量；启用前需配置对应账号的授权信息。",
        ["Miyoushe.Cookie"] = "供米游社账号数据请求使用的 Cookie 凭据；敏感值只返回遮蔽结果，不可把遮蔽占位符写回。",
        ["Miyoushe.LogSyncCookie"] = "是否与调度器日志使用的米游社 Cookie 配置同步；这是布尔开关，不是 Cookie 内容。",
        ["PathingConditionConfig.AvatarConditions"] = "地图追踪的角色条件规则集合，用于按队伍角色匹配追踪行为；复杂规则对象只读，不允许用不完整 JSON 重建。",
        ["GiWorldPosition.X"] = "从 position[2] 派生的横向世界坐标，只读；不是屏幕像素。",
        ["GiWorldPosition.Y"] = "从 position[0] 派生的纵向世界坐标，只读；position[1] 才是高度。",
        ["GiTpPosition.TranX"] = "从 tranPosition[2] 派生的实际传送落点横向世界坐标，只读。",
        ["GiTpPosition.TranY"] = "从 tranPosition[0] 派生的实际传送落点纵向世界坐标，只读。",
        ["GiTpPosition.Name"] = "复活用传送点的显示名称；仅名称不能确定实际坐标，不应单独改名来选择另一处神像。",
        ["GiTpPosition.Type"] = "复活用地图传送点的类型标识；应与所选神像点位记录保持一致。",
        ["GiTpPosition.Country"] = "复活用传送点所属国家或地区；应与点位坐标和名称保持一致。",
    };

    public static string? Find(Type owner, PropertyInfo property)
    {
        if (owner.Namespace?.StartsWith("BetterGenshinImpact.", StringComparison.Ordinal) != true) return null;
        if (property.Name == "HasErrors") return "BetterGI 配置验证器是否存在校验错误；这是只读诊断状态，不是可修改的开关。";
        for (var type = owner; type is not null; type = type.BaseType)
            if (Reviewed.TryGetValue(type.Name + "." + property.Name, out var guide)) return guide;
        if (owner.Name == "AutoLeyLineOutcropFightConfig")
        {
            // 特判先于通用回退：地脉花的 Timeout 与 KazuhaPickupEnabled 在参数转换中被覆盖或禁用，不能套用自动战斗的通用说明。
            if (property.Name == "Timeout") return "地脉花战斗配置转换为自动战斗参数时采用的超时，单位秒。";
            if (property.Name == "KazuhaPickupEnabled") return "保留的万叶拾取配置字段；当前地脉战斗参数转换显式禁用战后拾取，不应把此值当作地脉拾取已启用。";
            if (Reviewed.TryGetValue("AutoFightConfig." + property.Name, out var guide)) return "地脉花专用战斗配置：" + guide;
        }
        if (owner.Name == "MaskWindowConfig")
        {
            var panels = new Dictionary<string, string> { ["LogTextBox"] = "日志", ["StatusList"] = "任务状态", ["Metrics"] = "性能指标" };
            var axes = new Dictionary<string, string> { ["LeftRatio"] = "左边缘水平位置相对客户区宽度", ["TopRatio"] = "上边缘垂直位置相对客户区高度", ["WidthRatio"] = "宽度相对客户区宽度", ["HeightRatio"] = "高度相对客户区高度" };
            foreach (var (panel, label) in panels)
                foreach (var (axis, meaning) in axes)
                    if (property.Name == panel + axis) return $"{label}遮罩的{meaning}的比例，用于随游戏窗口尺寸计算布局；不是像素值。";
        }
        return null;
    }
}
