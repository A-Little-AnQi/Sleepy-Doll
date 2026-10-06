# 发布与更新

正式源码为当前 Git 仓库。普通提交触发检查，`v<版本>` 标签触发 Windows 安装包构建、R2 上传、GitHub Release 发布和更新通道切换。

## 地址与资源

- 官网（Vercel）：`https://sleepy-doll.restless-nh3.com`，源码 `E:\BetterGIProject\sleepy-doll-website`，公开仓库 `A-Little-AnQi/sleepy-doll-website`。
- 更新与统计 API（Cloudflare Worker）：`https://sleepy-doll-api.restless-nh3.com`。官网通过 Vercel `/api/*` 外部重写保留旧 API 地址，兼容已发布的 0.0.1。
- 安装包：`https://sleepy-doll-download.restless-nh3.com/releases/<版本>/Sleepy-Doll-<版本>-setup.exe`
- R2 桶：`sleepy-doll-releases`，Standard 存储。
- Worker：`sleepy-doll-distribution`。
- GA4：独立的 Sleepy Doll 媒体资源，衡量 ID `G-PBWX0V3ESR`。

发布属性由 `release-channel.json` 明确指定，当前 `0.1.0` 为 `stable` 正式版。`0.x` 不代表测试版，`1.x` 也不代表正式版。测试发布不会修改正式通道。

- `stable`：正式通道，GitHub Release 取消 Pre-release 并标记 Latest，不接受带预发布后缀的版本。
- `test`：测试通道，GitHub Release 标记 Pre-release，不标记 Latest。
- 支持 BetterGI 使用的 SemVer 后缀，例如 `0.2.0-alpha.1`，也支持 `-beta.1`、`-rc.1`。这些仅是命名示例，不指定下一正式版的版本号。
- 同一基础版本按 `alpha.2 < alpha.10 < beta.1 < rc.1 < 无后缀正式版本` 比较；构建信息 `+build` 不改变更新优先级。

## 发布步骤

1. 同步修改 `Cargo.toml`、`package.json` 和 `package-lock.json` 的产品版本。
   在 `release-channel.json` 选择 `stable` 或 `test`；带 Alpha、Beta、RC 后缀的版本必须选择 `test`。
2. 添加 `web/src/app/release-notes/changelog-<版本>.md`，并更新 `UpdateDialog.tsx` 引用。
3. 提交源码，推送分支，等待检查成功。
4. 创建并推送对应标签，例如 `v0.1.0`；标签发布读取仓库中的通道配置。手动运行 release 工作流时须明确选择发布通道，该选择同步写入构建中的通道配置，保证客户端、R2 清单与 GitHub 标记一致。
5. 安装包、SHA-256 和版本清单写入 `dist/`。工作流上传安装包并从公开地址取回校验。
6. 部署 Worker，发布 GitHub Release，最后写入 `channels/test.json` 或 `channels/stable.json`。

检查失败、上传失败或公开下载校验失败时不切换通道。同版本重复运行只接受相同安装包内容。发布全局串行执行，旧版本不能覆盖更新通道里的较新版本。

GitHub Actions Secrets：`R2_ACCESS_KEY_ID`、`R2_SECRET_ACCESS_KEY`、`CLOUDFLARE_API_TOKEN`、`GA_MEASUREMENT_ID`、`GA_API_SECRET`。凭据不进入源码、安装包或发布清单。

## 客户端行为

桌面程序启动后后台检查更新，手动检查与通道选择位于设置页「关于」标签（UI 入口：`web/src/pages/settings/AboutPage.tsx` + `web/src/app/ReleaseSettings.tsx`）。只接受 HTTPS 固定下载域名、正确版本路径和 SHA-256。下载安装包位于用户目录的 `.sleepy-doll/updates/`；安装前再次校验。

当前版本同时显示构建的正式／测试属性，与用户选择的更新通道分开。新安装默认使用构建自身的通道，已有用户保存的通道选择保留。正式通道只读取 `channels/stable.json`，测试通道只读取 `channels/test.json`；两者不静默互换。官网的主下载选择正式版，测试版使用独立入口，正式版缺失时不会悄悄改下载测试包。

安装更新要求当前程序位于注册安装目录，且没有正在执行的任务。安装器等待原进程退出，再调用原位置更新逻辑并重新启动。未登记的便携目录通过安装器更新。配置、会话和原有快捷方式保留。

安装缓存位于安装器所在目录的 `.cache/setup/`。验证工具的临时目录必须指向项目 `target/` 下的任务目录。

## 基础统计

客户端统计默认开启，新安装会在启动、检查更新和更新完成时上报匿名事件，业务埋点参数为版本、通道、平台、事件名、目标版本、`sessionId` 与随机 `clientId`，Google tag 还会附带标准会话和浏览器/设备信息，不含对话、模型密钥或文件路径。用户显式保存过 `analyticsEnabled=false` 的停用选择会被保留，修改更新通道不会改动统计开关。

客户端上报不再经过官网或 Worker：原生层把白名单事件经内存事件出口转交给界面，由界面内的官方 gtag.js（`https://www.googletagmanager.com/gtag/js?id=G-J691D7H7BY`）直连 Google Analytics。GA 资源 557227178 下使用独立的「桌面客户端」web 数据流（16041216887，测量 ID `G-J691D7H7BY`，`https://sleepy-doll.localhost`），增强型衡量关闭；上报带固定虚拟 `page_location=https://sleepy-doll.localhost/app`，这只是 GA 里的数据标签，客户端从不访问该网址；Google tag 直连无需任何 secret，客户端不保存任何 GA API 密钥，也不手工拼未公开的收集协议。`client_id` 沿用偏好文件里的随机 UUID，consent 中广告三项始终拒绝、analytics 仅在用户开启后授予，不发送页面浏览，也不读取对话、标题、URL 或文件路径。事件在界面就绪前最多缓冲 32 条、超过 30 秒丢弃；脚本加载失败或离线不影响任何功能，不弹提示、不重试。关闭统计（`analyticsEnabled=false`）后重启即完全停用：不加载脚本、置 `ga-disable` 标记，连无 Cookie ping 也不发送。以下统计描述仅适用于官网。

官网统计默认关闭，用户在网站主动同意后启用 GA4，发送匿名访问与下载点击事件。不发送对话、提示词、模型密钥、文件路径或用户文档。官网与 Worker 的统计链路保持原样，仅为兼容已发布的 0.0.1 客户端保留 `/api/events`，不因客户端直连而修改网站实现。

事件包括官网访问与下载点击。官网点击不代表文件下载完成，下载校验完成不代表安装完成。

Worker 校验事件白名单、字段格式、请求体大小和来源，并限流。GA 密钥仅保存在 Worker 服务端。统计接口失败不影响启动、下载或更新。测试通道事件启用 GA4 DebugView。

### 在 GA 中查看

资源：Sleepy Doll（557227178）。客户端直连事件进入独立的「桌面客户端」数据流（`G-J691D7H7BY`），与官网 web 数据流分开查看；旧版 0.0.1 经 Worker 上报的历史数据仍在原有探索里。报告导航「客户端统计 / 使用情况」中的「日活与启动」「版本与通道」「更新结果」。

「日活与启动」按 `app_start` 的日期分组：`totalUsers` 是去重匿名安装，`eventCount` 是启动次数，不含官网流量。它不是实名人数，也不符合 GA 的互动时长口径——跨日持续运行但没有再次启动的安装，不会计入新一天的启动日活，当前按「启动日活」理解。报告时区已核实为 GMT+08 中国时间。统计默认开启但用户可显式关闭，因此这些数字不代表全部安装量。

「更新结果」按 `update_check` / `update_available` / `update_download_complete` / `update_success` / `update_failed` 事件查看。数据里包含既有验证运行产生的记录，不能当作真实客户增长解读。

## 验收

GitHub 构建、公开下载哈希、软件更新状态和 GA4 实际事件分别核验。国内连通性需要关闭代理后实测；成功的单条线路不能代表所有地区和运营商。`verify-update` 工作流在 GitHub Actions 的 Windows 环境构建不发布的 0.0.0 基线，使用真实界面下载已发布的 0.0.1，执行原位置安装与重启，并校验配置、数据库、bridge 配置和标记文件保留。2026-10-04 首次验证通过，运行 ID 37136132020。

官网 main 提交由 Vercel 自动构建。分发服务单独由 `deploy-service` 部署，修改官网或 API 无需重新发布安装包。正式域名使用 Vercel 提供的 DNS-only CNAME，API 和下载分别使用一级子域名，避免多级通配证书问题。

## 下载费用保护

安装包下载由 Workers Free 的 `sleepy-doll-distribution` 读取私有 R2 桶，下载域名和版本路径不变。桶的 R2 自定义公共域名和 r2.dev 均关闭；下载域名作为 Worker Custom Domain 使用。

- 保持 Workers Free。当前账号每日动态请求上限 100,000，UTC 零点（北京时间 08:00）重置，耗尽后请求失败。
- 每个下载 GET/HEAD 最多执行一次 R2 get/head，不列目录、不重试；命中缓存则不读取 R2。更新清单每次请求也只读取一次。
- 下载仅接受规范版本路径和 GET/HEAD，不接受查询参数或多段 Range。每 IP 每分钟 120 次下载请求的绑定限流是辅助保护，计数按 Cloudflare 本地执行位置维护，不是精确全球计费限额。
- 在以上条件下，跨 UTC 日边界的 31 天窗口最多覆盖 32 个每日额度，公网 Worker 请求最多对应约 320 万次 R2 读取，低于 Standard 每月 1,000 万次 Class B 免费额度。CI、管理工具及其他桶的操作另计。
- Actions 发布前完整分页核对桶容量、存储类别和预计新增容量。超过 8,000,000,000 字节或存在非 STANDARD 对象即停止发布，不删除已有文件，不改变发布通道。上传明确使用 STANDARD。
- 如需手动暂停，在 Worker 的 Secrets 中设置 `DOWNLOADS_DISABLED=true`；仅暂停下载，GA 与更新检查仍由原接口提供。删除该 Secret 或设置 false 后恢复。

可在 Workers 的 Metrics 查看请求和错误，在 R2 统计页查看读取操作与存储。预算邮件有延迟，不参与请求放行决策。免费请求额度耗尽或受到攻击时，下载和更新检查可能暂时不可用，官网仍提供 GitHub Releases 备用下载。

保护范围是本发布链路的公网读取与受控 CI 上传；不保证整个账号零账单。升级 Workers Paid、启用付费产品、上传到其他桶或泄露管理凭据会改变这个边界。

下载域名另有免费 WAF 自定义规则 `Sleepy Doll - canonical download requests`：在 Worker 前阻断查询参数、GET/HEAD 以外的方法和非 `/releases/` 路径，减少明显异常请求消耗 Worker 免费额度。该规则由控制台管理，记录在 `docs/distribution-security.json`；现有部署 token 不包含 WAF 编辑权限。规则不使用验证码，避免阻断软件内下载。
