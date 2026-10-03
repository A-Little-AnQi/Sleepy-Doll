# 发布与更新

正式源码为当前 Git 仓库。普通提交触发检查，`v<版本>` 标签触发 Windows 安装包构建、R2 上传、GitHub Release 发布和更新通道切换。

## 地址与资源

- 官网（Vercel）：`https://sleepy-doll.restless-nh3.com`，源码 `E:\BetterGIProject\sleepy-doll-website`，公开仓库 `A-Little-AnQi/sleepy-doll-website`。
- 更新与统计 API（Cloudflare Worker）：`https://sleepy-doll-api.restless-nh3.com`。官网通过 Vercel `/api/*` 外部重写保留旧 API 地址，兼容已发布的 0.0.1。
- 安装包：`https://sleepy-doll-download.restless-nh3.com/releases/<版本>/Sleepy-Doll-<版本>-setup.exe`
- R2 桶：`sleepy-doll-releases`，Standard 存储。
- Worker：`sleepy-doll-distribution`。
- GA4：独立的 Sleepy Doll 媒体资源，衡量 ID `G-PBWX0V3ESR`。

`0.0.*` 属于测试通道，GitHub Release 标记为预发布。正式首版使用 `0.1.0`，属于正式通道。测试发布不会修改正式通道。

## 发布步骤

1. 同步修改 `Cargo.toml`、`package.json` 和 `package-lock.json` 的产品版本。
2. 添加 `web/src/app/release-notes/changelog-<版本>.md`，并更新 `UpdateDialog.tsx` 引用。
3. 提交源码，推送分支，等待检查成功。
4. 创建并推送对应标签，例如 `v0.0.1`；也可手动运行 release 工作流。
5. 安装包、SHA-256 和版本清单写入 `dist/`。工作流上传安装包并从公开地址取回校验。
6. 部署 Worker，发布 GitHub Release，最后写入 `channels/test.json` 或 `channels/stable.json`。

检查失败、上传失败或公开下载校验失败时不切换通道。同版本重复运行只接受相同安装包内容。发布全局串行执行，旧版本不能覆盖更新通道里的较新版本。

GitHub Actions Secrets：`R2_ACCESS_KEY_ID`、`R2_SECRET_ACCESS_KEY`、`CLOUDFLARE_API_TOKEN`、`GA_MEASUREMENT_ID`、`GA_API_SECRET`。凭据不进入源码、安装包或发布清单。

## 客户端行为

桌面程序启动后后台检查更新，设置页提供手动检查和通道选择。只接受 HTTPS 固定下载域名、正确版本路径和 SHA-256。下载安装包位于用户目录的 `.sleepy-doll/updates/`；安装前再次校验。

安装更新要求当前程序位于注册安装目录，且没有正在执行的任务。安装器等待原进程退出，再调用原位置更新逻辑并重新启动。未登记的便携目录通过安装器更新。配置、会话和原有快捷方式保留。

安装缓存位于安装器所在目录的 `.cache/setup/`。验证工具的临时目录必须指向项目 `target/` 下的任务目录。

## 基础统计

官网和软件统计默认关闭，用户主动开启后发送匿名基础事件。软件在设置页可以关闭。使用随机安装标识，不发送对话、提示词、模型密钥、文件路径或用户文档。

事件包括官网访问、下载点击、软件启动、更新检查、发现新版、软件下载校验完成、更新后首次启动成功和更新失败。官网点击不代表文件下载完成，下载校验完成不代表安装完成。

Worker 校验事件白名单、字段格式、请求体大小和来源，并限流。GA 密钥仅保存在 Worker 服务端。统计接口失败不影响启动、下载或更新。测试通道事件启用 GA4 DebugView。

## 验收

GitHub 构建、公开下载哈希、软件更新状态和 GA4 实际事件分别核验。国内连通性需要关闭代理后实测；成功的单条线路不能代表所有地区和运营商。`verify-update` 工作流在 GitHub Actions 的 Windows 环境构建不发布的 0.0.0 基线，使用真实界面下载已发布的 0.0.1，执行原位置安装与重启，并校验配置、数据库、bridge 配置和标记文件保留。2026-10-04 首次验证通过，运行 ID 37136132020。

官网 main 提交由 Vercel 自动构建。分发服务单独由 `deploy-service` 部署，修改官网或 API 无需重新发布安装包。正式域名使用 Vercel 提供的 DNS-only CNAME，API 和下载分别使用一级子域名，避免多级通配证书问题。

## 下载费用保护

安装包下载由 Workers Free 的 `sleepy-doll-distribution` 读取私有 R2 桶，下载域名和版本路径不变。桶的 R2 自定义公共域名和 r2.dev 均关闭；下载域名作为 Worker Custom Domain 使用。

- 保持 Workers Free。当前账号每日动态请求上限 100,000，UTC 零点（北京时间 08:00）重置，耗尽后请求失败。
- 每个下载 GET/HEAD 最多执行一次 R2 get/head，不列目录、不重试；命中缓存则不读取 R2。更新清单每次请求也只读取一次。
- 下载仅接受规范版本路径和 GET/HEAD，不接受查询参数或多段 Range。每 IP 每分钟 120 次下载请求的绑定限流是辅助保护，计数按 Cloudflare 本地执行位置维护，不是精确全球计费限额。
- 在以上条件下，31 天内公网 Worker 请求最多对应约 310 万次 R2 读取，低于 Standard 每月 1,000 万次 Class B 免费额度。CI、管理工具及其他桶的操作另计。
- Actions 发布前完整分页核对桶容量、存储类别和预计新增容量。超过 8,000,000,000 字节或存在非 STANDARD 对象即停止发布，不删除已有文件，不改变发布通道。上传明确使用 STANDARD。
- 如需手动暂停，在 Worker 的 Secrets 中设置 `DOWNLOADS_DISABLED=true`；仅暂停下载，GA 与更新检查仍由原接口提供。删除该 Secret 或设置 false 后恢复。

可在 Workers 的 Metrics 查看请求和错误，在 R2 统计页查看读取操作与存储。预算邮件有延迟，不参与请求放行决策。免费请求额度耗尽或受到攻击时，下载和更新检查可能暂时不可用，官网仍提供 GitHub Releases 备用下载。

保护范围是本发布链路的公网读取与受控 CI 上传；不保证整个账号零账单。升级 Workers Paid、启用付费产品、上传到其他桶或泄露管理凭据会改变这个边界。
