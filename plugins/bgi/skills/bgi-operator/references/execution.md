# 任务准备、执行与核验

## 资源目标

“跑个 X”先 bgi.user.resolve，用用户原话或明确目标；不先找本体命令、打开页面或遍历叶子路线。

- run：沿用返回的真实 groupName 和现有参数，读取 bgi.run_script_group 契约。仅处理工具证实的必要前提，不例行重配或读全部源码。
- repair：按 missing 补资源，再 resolve；不运行缺失引用的组。
- resourceFound：使用 repository.items 中的完整作者包，subscribe_script_resources 后 prepare_pathing_group，运行返回的组名；JS 按脚本定义处理。
- create：按候选类型准备；JS 用 inspect_script 读取 manifest 声明的参数定义，再 prepare_js_group；Pathing 不套 JS 流程。
- notFound：只代表本机与当前索引未命中；核对名称，确有更新需要再刷新一次。
- lookupFailed：按真实仓库错误处理，不说资源不存在。

一条龙用 bgi.run_one_dragon，不混同调度器组。只要求配置或封装快捷入口时，到保存与回读为止，不进入游戏启动。

## 调用与就绪

Agent 调用工具是 bgi.api.read/invoke，methodId 是工具参数，不是另一套工具名。describe 返回 agentCall 和 inputSchema；业务参数放 arguments 对象。readOnly 用 read，写动作用 invoke。callable 表示该接口契约可用，不等于游戏已就绪或任务已完成。

配置组运行接口标有 preparation=ensureGameReady 时，由执行器启动原神、挂接截图器并阻塞等待主界面；直接提交目标即可，不在模型中重复编排启动和轮询。其他确需游戏就绪而没有自动准备的接口，才按契约调用 start_game 和一次 wait_ready。wait_ready 不会自行启动不存在的游戏。

gameReady（旧字段 bridgeReady）只表示截图器和游戏主界面就绪，false 不表示 BetterGI 断连。连接错误以真实请求失败为准。非 16:9、目标分辨率超出当前桌面、缺少安装路径或人工验证等阻碍，按具体错误处理；不为了保存入口或读取配置启动游戏，也不擅自改分辨率。

## 返回与停止

普通运行与快捷入口默认 waitForCompletion=false，交接后总结并结束。多组及收尾关闭游戏用一次 run_script_groups 的 groupNames/closeGameAfter，不把计划顺序当成模型监控授权。用户明确要求等结果时才使用 true。工具等待期间不调用模型，返回已有终态不再 job.get，也不自动重跑。

accepted/verificationScope=launch 只证明启动交接；executed 只证明相应方法返回。业务完成以本次契约和实际证据为准。

停止尚未终态的原 Job 用 job.cancel；已交接的启动 Job 是回执，停止实际独立任务用 bgi.stop_current_task 并核对目标。timeout、取消请求已发出都不是停止完成。音乐、录制等使用自身停止入口。

## 确需原生界面时

直接资源、设置与业务入口优先；仅 UI 本身是目标或直接接口不适用时读 [native-ui.md](native-ui.md)。当前对象、selection、dialogInput 和 contextId 来自真实契约；不构造任意 CLR 类型、不扫描 DLL 猜接口。界面命令返回、窗口消失和 dialogsHandled 不证明业务完成，须回读目标。版本或对象已变时重新取证；弃用与空实现不恢复暴露。
