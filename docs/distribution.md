# 发布与更新

正式源码为当前 Git 仓库。普通提交触发检查，`v<版本>` 标签触发 Windows 安装包构建、R2 上传、GitHub Release 发布和更新通道切换。

## 地址与资源

- 官网（Vercel）：`https://sleepy-doll.restless-nh3.com`，源码见 [官网仓库](https://github.com/A-Little-AnQi/sleepy-doll-website)。
- 更新与统计 API（Cloudflare Worker）：`https://sleepy-doll-api.restless-nh3.com`。官网通过 Vercel `/api/*` 外部重写保留旧 API 地址，兼容已发布的 0.0.1。
- 安装包：`https://sleepy-doll-download.restless-nh3.com/releases/<版本>/Sleepy-Doll-<版本>-setup.exe`
- R2 桶：`sleepy-doll-releases`，Standard 存储。
- Worker：`sleepy-doll-distribution`。
- GA4：独立的 Sleepy Doll 媒体资源，衡量 ID `G-PBWX0V3ESR`。

发布属性由 `release-channel.json` 明确指定，构建时以该文件及产品版本清单为准。`0.x` 不代表测试版，`1.x` 也不代表正式版。测试发布不会修改正式通道。

- `stable`：正式通道，GitHub Release 取消 Pre-release 并标记 Latest，不接受带预发布后缀的版本。
- `test`：测试通道，GitHub Release 标记 Pre-release，不标记 Latest。
- 支持 BetterGI 使用的 SemVer 后缀，例如 `0.2.0-alpha.1`，也支持 `-beta.1`、`-rc.1`。这些仅是命名示例，不指定下一正式版的版本号。
- 同一基础版本按 `alpha.2 < alpha.10 < beta.1 < rc.1 < 无后缀正式版本` 比较；构建信息 `+build` 不改变更新优先级。

## 发布步骤

1. 同步修改 `Cargo.toml` 和 `package.json` 的产品版本。
   在 `release-channel.json` 选择 `stable` 或 `test`；带 Alpha、Beta、RC 后缀的版本必须选择 `test`。
2. 有更新说明时添加 `web/src/app/release-notes/changelog-<版本>.md`，构建自动读取当前版本；修复版可不提供说明，发布清单和 GitHub Release 保持空正文，客户端不展示空日志。
3. 提交源码，推送分支，等待检查成功。
4. 创建并推送对应标签，例如 `v0.1.0`；标签发布读取仓库中的通道配置。手动运行 release 工作流时须明确选择发布通道，该选择同步写入构建中的通道配置，保证客户端、R2 清单与 GitHub 标记一致。
5. 安装包、SHA-256 和版本清单写入 `dist/`。工作流上传安装包并从公开地址取回校验。
6. 部署 Worker，发布 GitHub Release，最后写入 `channels/test.json` 或 `channels/stable.json`。

检查失败、上传失败或公开下载校验失败时不切换通道。同版本重复运行只接受相同安装包内容。发布全局串行执行，旧版本不能覆盖更新通道里的较新版本。

安装包上传并校验、分发服务部署均已完成，但 GitHub 发布或通道切换失败时，可手动运行 `release`，填写原 `uploaded_run` 和 `uploaded_sha256` 续接。流程会核对原检查结果、构建提交、版本标签、清单与公开下载字节，只复用原包，不重新构建或覆盖不同内容；不满足这些条件时拒绝续接。

### 明确授权的同版热修复

只有用户明确要求覆盖已发布包体时，使用 `replace-release` 工作流；普通 `release` 仍禁止不同内容覆盖同版本。提供精确的产品源码提交 `source_commit`、当前正式包的 SHA-256 和本次修复说明 `note`，工作流在单一检出目录构建该提交，只从工具提交加载覆盖脚本，不混入本机未提交的功能开发。

覆盖前核对原包、校验文件、归档清单、GitHub 资产和存储容量，仅暂停目标版本的下载。同步替换 R2 与 GitHub 资产、正式清单并验证公开下载的新 SHA-256 后，以原引用校验更新版本标签到产品修复提交；失败恢复原资产、清单和标签，恢复不完整时保持下载暂停并报告。GitHub Actions 使用自身令牌更新标签，避免再次触发普通发布。

同版热修复不改变版本比较结果：已安装该版本的客户端不会因包体变化再次提示版本升级，需要重新安装；旧版客户端检查更新会取得替换后的安装包与校验值。

GitHub Actions Secrets：`R2_ACCESS_KEY_ID`、`R2_SECRET_ACCESS_KEY`、`CLOUDFLARE_API_TOKEN`、`GA_MEASUREMENT_ID`、`GA_API_SECRET`。凭据不进入源码、安装包或发布清单。

## Windows 自签名

本机首次执行 `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/sign-windows.ps1 -Action Initialize`，创建 RSA 3072 / SHA-256 代码签名证书。证书固定保存在当前用户 Windows 证书库 `Cert:\CurrentUser\My`，私钥不可导出，不写入仓库、安装载荷或发布清单。重复执行复用原证书；公开证书导出到 `target/release/Sleepy-Doll.cer`，仅含公钥。不要删除或重建证书来处理普通构建失败。

之后 `npm run dist` 自动复用名为 `Sleepy Doll Code Signing` 的证书：先签名暂存的主程序及桥 EXE/DLL，再压缩进安装载荷；最后签名安装器，再计算 SHA-256、体积与本地未发布清单。签名采用 DigiCert RFC 3161 时间戳，网络失败、证书失效、签名缺失或摘要不匹配均中止组包。构建缓存中的主程序与桥保持原样。`-Action Verify -Path <target 内的 EXE/DLL>` 可复核文件签名与当前签名身份。

本机回归入口：`powershell -NoProfile -ExecutionPolicy Bypass -File scripts/sign-windows.test.ps1`。它使用已构建的桥 EXE 验证签名、时间戳、篡改拒绝、无签名拒绝、指定证书失败和源码范围拒绝；临时文件只放在 `target/.tmp/windows-signing/` 并在结束时清理，不创建证书或安装可信根。

可以通过 `SLEEPY_DOLL_SIGN_CERT_SHA1` 指定当前用户证书库中的其他代码签名证书；变量中的 SHA-1 只是证书指纹，文件摘要仍是 SHA-256。指定了无效证书时禁止回退为无签名包。未配置签名证书的机器（包括目前的 CI）会明确打印无签名构建提示。CI 需要单独配置可用于该 runner 的固定签名身份；本机不可导出私钥不会自动上传，也不在 CI 每轮生成新身份。

自签名不等于受 Windows 公共信任：本流程不自动安装可信根、不修改目标机器的信任设置。未单独信任公钥的电脑上可能显示未知发布者／信任警告；签名脚本只放行 Windows 校验的成功结果或自签名证书的“不受信任根”错误 `0x800B0109`，其余错误全部拒绝。后者只能说明签名完整性通过，不能宣称系统已信任。PowerShell 可能把该信任错误显示为 `NotTrusted` 或 `UnknownError`，不能只凭这个概括状态判断签名。需要目标电脑信任时，先通过独立渠道核对证书指纹，再由使用者决定是否导入。自签名本身也不建立 SmartScreen 信誉。参见 [Microsoft SignTool 文档](https://learn.microsoft.com/en-us/windows/win32/seccrypto/signtool) 与 [证书信任说明](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_signing)。

本地重签会改变安装包哈希，必须连同校验文件与未发布清单一起重新生成；不能直接覆盖线上同版本资产，线上替换仍遵守前述同版热修复流程。

## 客户端行为

安装器使用 Slint 原生界面，启动时请求管理员权限并居中显示。缺少 WebView2 或版本低于要求时，调用微软 Evergreen 安装程序进行整机安装或升级，完成后重新检测；下载失败页提供官方下载链接与重试。安装器本身不依赖 WebView2。

桌面程序启动后后台检查更新，手动检查与通道选择位于设置页「关于」标签（UI 入口：`web/src/pages/settings/AboutPage.tsx` + `web/src/app/ReleaseSettings.tsx`）。只接受 HTTPS 固定下载域名、正确版本路径和 SHA-256。下载安装包位于用户目录的 `data/updates/`；安装前再次校验。

当前版本同时显示构建的正式／测试属性，与用户选择的更新通道分开。新安装默认使用构建自身的通道，已有用户保存的通道选择保留。正式通道只读取 `channels/stable.json`，测试通道只读取 `channels/test.json`；两者不静默互换。官网的主下载选择正式版，测试版使用独立入口，正式版缺失时不会悄悄改下载测试包。

安装更新要求当前程序位于注册安装目录，且没有正在执行的任务。安装器等待原进程退出，再调用原位置更新逻辑并重新启动。未登记的便携目录通过安装器更新。配置、会话和原有快捷方式保留。

同版本仍按原位置更新处理。卸载删除程序及安装目录中的用户数据，同时清理旧版 `bridge/user/` 和漫游回退目录，不提供保留数据选项。

安装缓存位于安装器所在目录的 `.cache/setup/`。验证工具的临时目录必须指向项目 `target/` 下的任务目录。

## 基础统计与数据边界

客户端的匿名统计默认开启，保留用户已经保存的停用选择；设置页可关闭。事件覆盖启动、更新检查、下载校验和更新结果，包括版本、通道、平台、随机安装标识与会话标识，不发送对话、模型密钥、文档或本机文件路径。

客户端通过官方 gtag.js 直连独立的 Google Analytics 数据流，不持有 GA 服务端密钥；关闭后重启不会加载统计脚本。事件队列有上限和过期时间，离线或加载失败不影响产品功能。官网与 Worker 使用各自的同意和事件校验流程；服务端密钥只放在部署环境。

客户端、官网和旧版兼容事件须分别分析。启动次数不等于活跃用户，下载点击不等于完成下载，下载校验通过不等于安装完成。测试运行也可能产生事件，不能当作真实用户增长。部署使用的数据流、资源编号与控制台报表属于运营配置，不作为仓库中的现状快照维护。

## 验收

GitHub 构建、公开下载哈希、软件更新状态和 GA4 实际事件分别核验。国内连通性需要关闭代理后实测；成功的单条线路不能代表所有地区和运营商。安装更新应在隔离 Windows 环境验证原位置安装、重启及配置、数据库、桥令牌的保留。曾经固定验证 0.0.0 → 0.0.1 的一次性工作流已移除；当前仓库没有自动执行最新版桌面更新全流程的 CI 任务，不能用历史结果代替本次版本验收。

官网 main 提交由 Vercel 自动构建。分发服务单独由 `deploy-service` 部署，修改官网或 API 无需重新发布安装包。正式域名使用 Vercel 提供的 DNS-only CNAME，API 和下载分别使用一级子域名，避免多级通配证书问题。

## 下载费用保护

安装包下载由 Workers Free 的 `sleepy-doll-distribution` 读取私有 R2 桶，下载域名和版本路径不变。桶的 R2 自定义公共域名和 r2.dev 均关闭；下载域名作为 Worker Custom Domain 使用。

- 保持 Workers Free。本部署按每日 100,000 次动态请求预算设计；平台实际额度与计费以账号和当前服务条款为准，不能把本文当作实时额度证明。
- 每个下载 GET/HEAD 最多执行一次 R2 get/head，不列目录、不重试；命中缓存则不读取 R2。更新清单每次请求也只读取一次。
- 下载仅接受规范版本路径和 GET/HEAD，不接受查询参数或多段 Range。每 IP 每分钟 120 次下载请求的绑定限流是辅助保护，计数按 Cloudflare 本地执行位置维护，不是精确全球计费限额。
- 在以上条件下，跨 UTC 日边界的 31 天窗口最多覆盖 32 个每日额度，公网 Worker 请求最多对应约 320 万次 R2 读取，低于 Standard 每月 1,000 万次 Class B 免费额度。CI、管理工具及其他桶的操作另计。
- Actions 发布前完整分页核对桶容量、存储类别和预计新增容量。超过 8,000,000,000 字节或存在非 STANDARD 对象即停止发布，不删除已有文件，不改变发布通道。上传明确使用 STANDARD。
- 如需手动暂停，在 Worker 的 Secrets 中设置 `DOWNLOADS_DISABLED=true`；仅暂停下载，GA 与更新检查仍由原接口提供。删除该 Secret 或设置 false 后恢复。

可在 Workers 的 Metrics 查看请求和错误，在 R2 统计页查看读取操作与存储。预算邮件有延迟，不参与请求放行决策。免费请求额度耗尽或受到攻击时，下载和更新检查可能暂时不可用，官网仍提供 GitHub Releases 备用下载。

保护范围是本发布链路的公网读取与受控 CI 上传；不保证整个账号零账单。升级 Workers Paid、启用付费产品、上传到其他桶或泄露管理凭据会改变这个边界。

下载域名另有免费 WAF 自定义规则 `Sleepy Doll - canonical download requests`：在 Worker 前阻断查询参数、GET/HEAD 以外的方法和非 `/releases/` 路径，减少明显异常请求消耗 Worker 免费额度。该规则由部署方在控制台管理：限定下载主机、GET/HEAD、空查询串及 `/releases/` 路径。规则 ID、启用状态和当前 token 权限应在部署现场核对，不在源码中维护控制台快照。规则不使用验证码，避免阻断软件内下载。
