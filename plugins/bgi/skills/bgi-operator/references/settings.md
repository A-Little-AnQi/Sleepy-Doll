# 全局设置与联动事务

## 修改全局设置

`User\config.json` 和 BetterGI 安装目录里的宿主配置只能通过桥的设置事务修改，禁止用 `workspace.write`、`workspace.shell` 或任何本机文件命令改它们。运行中的 BetterGI 会用内存值覆盖磁盘，而且字段 setter 可能有联动行为。用户没有开发环境，不要为此安装中间件。

固定流程：

1. `bgi.api.search` 在 `settings` 组用一个核心业务词定位候选。
2. 只对最可能的候选调用一次 `bgi.api.describe`。
3. 用 `bgi.api.read` 读取当前值、`valueSchema`、`writable` 和 `valueVersion`。
4. 单字段可用 `bgi.set_setting`；多字段用 `bgi.preview_settings` 后提交 `bgi.commit_settings`。
5. 运行时按实际改动范围决定要不要确认一次写操作。提交后回读；保存 `changeId` 供回退。
6. 候选不可写或语义不确定时停止修改并说明具体缺口，不靠字段名猜效果。
