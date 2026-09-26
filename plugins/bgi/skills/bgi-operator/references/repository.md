# 仓库更新与订阅

## 更新脚本仓库与订阅内容

BGI 操作不使用 `workspace.*` 扫描桥缓存、DLL、EXE 或反射方法来发现能力。已知稳定入口直接 describe；目录返回不可调用就报告具体限制，不靠程序集文本、手工复制路线或别的接口绕过。

1. `bgi.update_subscribed_scripts` 是固定的稳定接口，直接 `bgi.api.describe`，不先调用 `bgi.api.search`。用户只要求刷新中央脚本仓库时使用 `repositoryOnly`；不要用“打开脚本仓库”代替更新。
2. 用户要求更新某个已安装脚本或路线时，读取 `User/Subscriptions` 中当前订阅文件，把目标解析为其中真实的订阅路径。名称对应不唯一时才询问。
   “更新/升级/同步某脚本”默认指订阅内容；除非用户明确说配置组或一条龙流程，否则不并行查询 `OneDragon`、`ScriptGroup`。
3. 调用 `bgi.update_subscribed_scripts`：传 `paths` 时只更新这些已订阅路径；省略时更新全部订阅。接口会先同步当前渠道的中央仓库。
4. 仓库/脚本更新不依赖截图器、游戏句柄或前台窗口，不调用 `bgi.state.get`，也不读取自动更新周期、上次更新时间等设置来代替执行。
5. 更新会覆盖订阅资源的程序文件，但沿用 BetterGI 自带的脚本配置保留逻辑。Job 完成后回读目标脚本的 manifest 或关键文件；不要只凭“处理器返回”声称版本已更新。
6. `bgi.api.invoke` 已跟踪 Job 到终态。返回中已经有 completed、failed 或 cancelled 时直接处理 evidence，不再调用 `bgi.job.get`；只有恢复中断任务且手里只有 Job ID 时才查询。
