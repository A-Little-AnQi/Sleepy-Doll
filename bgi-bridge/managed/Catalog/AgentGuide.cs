using System.Text.Json;

namespace BgiBridge.Catalog;

public sealed record AgentGuide(
    string Title, string Purpose, string[] WhenToUse, string[] Preconditions,
    string[] SideEffects, string ResultMeaning, string Verification, string Rollback,
    JsonElement[] Examples, string DocumentationSource = "bridge-contract", string? SourceReference = null);

public static class AgentGuides
{
    public static AgentGuide For(string id)
    {
        var (title, purpose, when, result, verify, rollback, example) = id switch
        {
            "bgi.ping" => ("桥连通性", "检查当前 token 能否访问注入桥。不读取游戏画面。", "仅用于连接诊断；正常业务接口已经返回时无需调用", "ok=true 仅表示桥响应。hostLoaded 不代表截图器或游戏就绪。", "需要执行环境时读取 bgi.get_status。", "只读。", "{}"),
            "bgi.probe" => ("BetterGI 兼容性", "检查桥依赖的 BetterGI 类型、DI 容器和 UI 调度器。", "接口报告成员缺失或版本不兼容时", "每个探测项独立表示可用、缺失或不可达。", "按目标接口所需依赖判断；不把探测成功当作任务成功。", "只读。", "{}"),
            "bgi.get_status" => ("BetterGI 运行状态", "读取截图器、游戏句柄、窗口前台状态和独立任务锁。", "执行前、执行后或排障时；纯 User 文件操作不调用", "一次带时间戳的状态快照。ready 不是业务完成标记。", "检查 observedAt；执行结果按目标接口另行核验。", "只读。", "{}"),
            "bgi.start_game" => ("启动原神", "让 BetterGI 按「联动启动」的配置拉起原神并开始截图，随后返回最新状态。", "游戏或截图器没就绪、而用户要求运行游戏任务时（用户说「启动游戏」「打开原神」也是这个意思）；由你调用，不要反过来要求用户自己启动", "started 表示已经发出启动；ready 才是可以执行任务。stillLoading=true 是加载中的过程状态，不是失败。BetterGI 拒绝启动时本接口直接失败并说明缺哪一项，那类错误要把缺项讲给用户。", "用 bgi.get_status 等到 ready=true，再执行原任务。", "只启动进程与截图器；不改变已保存的配置。", "{}"),
            "bgi.list_setting_sections" => ("设置分区", "列出 AllConfig 的设置分区及可读、可写、敏感字段数量。", "尚不知道设置属于哪个分区时", "分区摘要，不包含设置当前值。", "选定分区后调用 bgi.search_settings。", "只读。", "{}"),
            "bgi.search_settings" => ("查找设置", "按业务词、路径、分区和值类型查找 AllConfig 设置；不搜索配置组、脚本或路线。", "需要把设置需求映射到精确 path 时", "items 包含当前值摘要、写入能力和 valueVersion。", "只对选中的 path 再调用 bgi.get_setting。", "只读。", """{"terms":["拾取"],"limit":20}"""),
            "bgi.get_setting" => ("读取设置", "读取一个精确 path 的当前值、Schema、写入限制和并发版本。", "修改前取最新版本，或提交后回读", "currentValue 是当前内存值；敏感值被遮蔽。", "保存 valueVersion；按 valueSchema 生成新值。", "只读。", """{"path":"triggerInterval"}"""),
            "bgi.preview_settings" => ("预览多项设置", "校验 1–20 项变更并生成绑定当前内存和磁盘版本的计划，不写配置。", "一次修改多个设置时", "differences 是将要提交的脱敏差异；planId 十分钟有效。", "差异与用户目标一致后提交同一 planId。", "预览不需要回退。", """{"changes":[{"path":"triggerInterval","value":50,"expectedVersion":"get_setting 返回的 valueVersion"}]}"""),
            "bgi.commit_settings" => ("提交设置计划", "提交 preview_settings 生成的计划，创建恢复记录并原子写入。", "预览差异已经符合目标时；运行时审批负责确认", "verified=true 才表示内存和磁盘均已核验。changeId 标识本次事务。", "回读目标 path，并保留 changeId。", "用 bgi.rollback_settings；字段后来被修改时回退会拒绝覆盖。", """{"planId":"preview_settings 返回的 planId"}"""),
            "bgi.set_setting" => ("修改单项设置", "使用最新 expectedVersion 修改一个设置，执行完整备份、提交和核验。", "只改一个已读取的设置时，代替 preview+commit", "verified=true 表示本次单项提交已核验。", "回读目标 path；保留 changeId。", "用 bgi.rollback_settings。", """{"path":"triggerInterval","value":50,"expectedVersion":"get_setting 返回的 valueVersion"}"""),
            "bgi.list_setting_changes" => ("设置变更记录", "列出桥创建的配置事务和命令前检查点，不返回敏感原文。", "查找 changeId 或排查未完成事务时", "prepared 或 recoveryRequired 需要进一步检查。", "对目标 changeId 调用 bgi.get_setting_change。", "只读。", "{}"),
            "bgi.get_setting_change" => ("设置变更详情", "读取一个 changeId 的脱敏差异、状态和恢复记录。", "解释某次修改或准备回退时", "只描述该事务涉及的字段。", "核对 changeId、状态和目标 path。", "只读；回退使用 bgi.rollback_settings。", """{"changeId":"list_setting_changes 返回的 changeId"}"""),
            "bgi.rollback_settings" => ("回退设置事务", "仅在目标字段仍等于该事务提交值时恢复旧值；保留其他后续修改。", "用户要求撤销已确定的 changeId 时；运行时审批负责确认", "verified=true 表示回退后的内存与磁盘已核验。", "回读目标 path。CONFIG_CONFLICT 表示未覆盖后续修改。", "回退本身生成新记录。", """{"changeId":"提交结果的 changeId"}"""),
            "bgi.list_commands" => ("界面命令目录", "列出低层 WPF 命令的目标、参数、可调用状态和影响。命令通常作用于界面当前选择。", "确实需要界面动作且尚无精确 command ID 时", "目录项不是用户资源，也不证明当前界面选择正确。", "选择一个命令后读取其独立契约。", "只读。", """{"filter":"运行","includeDangerous":false}"""),
            "bgi.invoke_command" => ("执行界面命令", "执行已确定且当前可调用的界面命令；异步命令等待处理器返回。", "命令目标、界面上下文和参数都已确认时；运行时审批负责确认", "executed=true 只表示处理器返回，configurationCheckpoint 只保护配置。", "按具体命令契约读取状态或资源；超时后不盲目重发。", "配置可从检查点恢复；文件、通知、进程和游戏副作用通常不能自动撤销。", """{"command":"list_commands 返回的精确 name"}"""),
            _ => throw new InvalidOperationException($"接口 {id} 必须提供完整 AgentGuide。"),
        };

        var sideEffects = id switch
        {
            "bgi.commit_settings" or "bgi.set_setting" or "bgi.rollback_settings" =>
                new[] { "修改 BetterGI 内存配置并原子更新 User/config.json。" },
            "bgi.invoke_command" =>
                new[] { "影响由具体命令决定；以该命令独立契约为准。" },
            "bgi.start_game" =>
                new[] { "按 BetterGI 已配置的安装路径启动原神进程，并启动截图器与实时触发器；BetterGI 自身配置可能同时改注册表关闭原神 HDR。" },
            _ => new[] { "无写入副作用。" },
        };
        return new(
            title,
            purpose,
            [when],
            ["桥已连接；目标接口的 callable=true。"],
            sideEffects,
            result,
            verify,
            rollback,
            [ArgumentSchema.Parse(example)]);
    }
}
