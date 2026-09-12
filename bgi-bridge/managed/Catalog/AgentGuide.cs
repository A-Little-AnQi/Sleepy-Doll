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
            "bgi.ping" => ("检查桥服务", "确认鉴权后的桥请求能够到达当前 BetterGI 进程。不会读取截图或操作游戏。", "连接建立后检查桥的存活状态", "ok 表示桥可响应；hostLoaded 只表示宿主程序集可见，不表示游戏已就绪。", "需要游戏状态时继续调用 bgi.get_status。", "只读，无需回退。", "{}"),
            "bgi.probe" => ("诊断宿主兼容性", "检查桥依赖的宿主类型、服务容器和界面调度器是否可用，帮助定位版本不匹配。", "连接存在但接口报成员缺失或宿主未就绪时", "逐项诊断结果；缺失项是兼容性证据，不能据此推断游戏任务成功。", "确认所需类型和 uiDispatcherAvailable 可用，再调用目标接口。", "只读，无需回退。", "{}"),
            "bgi.get_status" => ("读取运行状态", "读取宿主、截图器、游戏窗口及独立任务状态，不触发按键、点击或任务启动。", "规划操作前建立新鲜状态，或执行后核对运行环境", "ready 与 runtime 是一次观测；没有位置信息时必须保留未知。", "使用 observedAt 判断新鲜度；不要把 ready 当作业务目标已完成。", "只读，无需回退。", "{}"),
            "bgi.list_setting_sections" => ("浏览配置分区", "列出当前宿主可发现的配置分区，以及每区可读、可写和敏感字段数量。", "不确定某项功能归属哪个配置分区时", "sections 中每项是分区摘要，不是当前配置内容。", "使用分区名调用 bgi.search_settings。", "只读，无需回退。", "{}"),
            "bgi.search_settings" => ("检索配置项", "按用途词、路径、分区和值类型定位配置项，返回脱敏值、约束及用于并发校验的 valueVersion。", "把自然语言配置需求映射到精确设置路径时", "items 是当前匹配的设置契约；writable=false 的原因在 writeRestriction。", "修改前对目标路径调用 bgi.get_setting 获取最新 valueVersion。", "只读，无需回退。", """{"terms":["拾取"],"limit":20}"""),
            "bgi.get_setting" => ("读取单项配置契约", "按目录提供的精确路径读取当前值、类型、用途、允许范围、敏感标记与值版本。不接受任意对象属性路径。", "准备修改一个已发现的配置项，或验证修改和回退结果时", "currentValue 是当前值；敏感字段只返回遮蔽值，valueVersion 仍可用于并发校验。", "保存 valueVersion，用于后续 preview_settings 或 set_setting。", "只读，无需回退。", """{"path":"triggerInterval"}"""),
            "bgi.preview_settings" => ("预览配置变更", "验证最多 20 项设置的类型、范围、可写性和旧值版本，生成限时计划及脱敏差异；不修改宿主配置。", "用户要求修改设置时先预览，尤其涉及多项联动修改时", "planId 绑定预览时的内存与磁盘状态；differences 列出预期变化。", "检查差异符合用户目标，再用 planId 提交；配置变化后旧计划失效。", "预览不产生配置修改，无需回退。", """{"changes":[{"path":"triggerInterval","value":50,"expectedVersion":"从 get_setting 返回值复制"}]}"""),
            "bgi.commit_settings" => ("提交已预览配置", "提交已验证的配置计划。写入前保存恢复记录，暂停本次赋值的自动保存，原子更新配置文件并核验内存和磁盘。", "用户已授权并已检查 preview_settings 的差异后", "返回 changeId、verified 和新版本；只有 verified=true 才可报告保存完成。", "再次读取目标设置；保留 changeId 以便回退。", "调用 bgi.rollback_settings；若目标项被用户另行修改会拒绝覆盖。", """{"planId":"从 preview_settings 返回值复制"}"""),
            "bgi.set_setting" => ("安全修改单项配置", "单项配置的便捷事务入口，要求最新 expectedVersion，执行与预览/提交同样的备份、校验和回退流程。", "只修改一个已读取契约的配置项时", "返回 changeId 和 verified；不是直接反射 setter 后宣称已保存。", "要求 verified=true，并可用 get_setting 复查当前值。", "使用 changeId 调用 bgi.rollback_settings。", """{"path":"triggerInterval","value":50,"expectedVersion":"从 get_setting 返回值复制"}"""),
            "bgi.list_setting_changes" => ("查看配置变更记录", "列出桥创建的配置事务、状态、修改路径及恢复记录标识，不返回敏感配置原文。", "排查最近配置变化，或寻找需要撤销的 changeId 时", "changes 是持久化变更摘要；prepared/recoveryRequired 表示上次操作需要核对。", "按 changeId 查看具体记录，再决定是否回退。", "只读，无需回退。", "{}"),
            "bgi.get_setting_change" => ("读取配置变更详情", "读取一个桥配置事务的脱敏差异、核验状态和备份位置。", "用户询问改了什么，或准备回退指定事务时", "差异只包含本事务涉及的路径；敏感值始终遮蔽。", "确认记录与当前目标一致，禁止猜测 changeId。", "此接口只读；撤销使用 bgi.rollback_settings。", """{"changeId":"从变更记录复制"}"""),
            "bgi.rollback_settings" => ("回退配置变更", "仅当目标字段仍等于该事务提交值时恢复旧值，并保留其他字段的后续修改。冲突时拒绝强制覆盖。", "用户要求撤销某次桥配置变更，且已确认 changeId 时", "返回 verified 与恢复后的版本；冲突不会修改用户当前值。", "读取目标配置验证恢复；失败时根据 recoveryRequired 和备份路径处理。", "回退同样生成记录；不覆盖其他来源的更新。", """{"changeId":"从提交结果复制"}"""),
            "bgi.list_commands" => ("浏览宿主命令", "列出宿主命令的用途、参数类型、可调用条件与副作用边界。这里只发现命令，不创建 ViewModel 或执行命令。", "已有专用接口无法覆盖目标，需寻找宿主界面命令时", "commands 包含可调用状态与阻止原因；发现命令不等于命令当前 CanExecute。", "先阅读命令契约，再调用；删除、导入等不可逆副作用不能由配置回退替代。", "只读，无需回退。", """{"filter":"settings","includeDangerous":true}"""),
            "bgi.invoke_command" => ("调用宿主命令", "调用已发现命令并检查 CanExecute；异步命令等待返回。仅支持明确可序列化的参数，遵守命令分组和禁用列表。", "专用接口没有对应能力，且用户明确授权该命令及副作用时", "executed 仅证明命令返回，不证明游戏目标完成；未知效果必须继续观测。", "根据命令说明检查结果；不得因网络超时盲目再次执行。", "命令的外部副作用通常不可自动回退；配置修改优先使用事务接口。", """{"command":"从 list_commands 返回的 name 复制"}"""),
            _ => throw new InvalidOperationException($"接口 {id} 必须提供完整 AgentGuide。"),
        };
        return new(title, purpose, [when], ["桥已连接且当前接口可调用。"],
            id is "bgi.commit_settings" or "bgi.set_setting" or "bgi.rollback_settings" ? ["修改宿主配置并刷新相关配置观察者。"]
            : id == "bgi.invoke_command" ? ["取决于具体命令，可能启动任务、打开界面、写文件或退出宿主。"] : ["不修改宿主配置或游戏状态。"],
            result, verify, rollback, [ArgumentSchema.Parse(example)]);
    }
}
