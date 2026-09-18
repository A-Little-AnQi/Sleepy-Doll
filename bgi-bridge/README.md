# BgiBridge

把 Sleepy Doll 的控制面注入 BetterGI 进程，让原版 BetterGI 提供 `/bridge/v1` 接口。

不修改宿主源码、不写入宿主目录、不修改宿主配置；宿主进程退出后不残留任何改动。

## 发布布局

发布版把桥组件收在安装目录的 `bridge\` 子目录里：`bridge\` 内是 9 个 `BgiBridge.*` 文件与
`bridge.config.json`，主程序 `sleepy-doll.exe` 在上一级。这一整目录必须保持在一起，主程序按
同样的相对位置查找它们。

## 目录

| 目录 | 内容 |
|---|---|
| `native/` | 注入器与引导 DLL。C++，只依赖 kernel32 |
| `managed/` | 桥本体。C#，运行于 BetterGI 进程内，无第三方依赖 |
| `recovery/` | 离线恢复工具，随发布一起分发 |
| `dev/` | 开发期专用：契约测试、元数据生成器、本地脚本 |

## 构建

```cmd
build.cmd
```

产物输出到仓库的 `target\bridge\`，四个 .NET 项目的中间目录在 `target\dotnet\` 下。
本目录不留构建产物。通常无需单独执行，根目录的 `build-desktop.cmd` 会调用本脚本。

## 配置

`bridge.config.json`（模板见 `bridge.config.example.json`）：

| 字段 | 作用 |
|---|---|
| `enabled` | 总开关。关掉后注入器不注入，已注入的实例对请求回 503 |
| `listen` | 监听地址，只允许回环 |
| `token` | 鉴权。留空时由 Sleepy Doll 首次连接时生成并写入 |
| `groups` | 按组开关。关掉的组不进 catalog，invoke 也拒绝 |
| `disabledMethods` | 单方法黑名单，优先级最高 |
| `userDirectory` | 数据根。由 Sleepy Doll 每次连接写入，指向安装目录下的 `user\`；缺失时退回桥目录下的 `user\` |

## 运行期数据

桥不改宿主目录，也不往安装目录里堆运行期产物：它的数据落在 Sleepy Doll 安装目录的
`user\` 下，落点集中在 `managed/InstallPaths.cs`。

| 路径 | 内容 |
|---|---|
| `user\log\bridge.log` | 桥在宿主进程内的日志。宿主是 WPF 应用，没有属于桥的控制台 |
| `user\.sleepy-doll\config-changes` | 宿主配置改动记录，供回滚与重装后追溯 |

`user\` 是用户的目录：安装程序不覆盖它，卸载默认也保留（只在检测到该目录时问一次是否连同
删除），所以这些记录在重装之后仍然可用。
目录建不出来时桥不中断运行：日志退回临时目录，改动记录写不进去由调用方自己报错。

## 开发

```cmd
dev\dev-rebuild.cmd    退出占用 DLL 的进程后重建
dev\dev-cycle.cmd      完整循环：重建、启动测试用 BetterGI、注入
```

已加载的 DLL 会锁定文件，重建前必须先退出宿主；`dev\dev-rebuild.cmd` 已包含该步骤。

## 实现约束

修改前请先阅读以下四条，均为已验证必须遵守的约束：

1. **入口在 `DllMain`，不在导出函数。** 宿主是启用 CFG 的 .NET 程序，
   `CreateRemoteThread` 指向刚映射进来的模块会被 `0xC0000409/0xA` 拦下并杀掉宿主。
   `DllMain` 由加载器调用，不受该限制。因此注入器只执行一次 `LoadLibraryW`。
2. **释放远程参数前必须等线程结束。** `CreateRemoteThread` 只是创建线程，
   提前 `VirtualFreeEx` 会让 `LoadLibraryW` 读到已解映射的页。
3. **托管依赖只能用宿主 TPA 里已有的。** 嵌入的 `BetterGI.deps.json` 不可编辑，
   桥的依赖无法加入宿主的依赖图。因此只使用 `System.Net.HttpListener` 和 `System.Text.Json`。
4. **宿主程序集叫 `BetterGI`，不是 `BetterGenshinImpact`。** 后者是命名空间。
