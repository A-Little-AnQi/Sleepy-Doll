# BgiBridge

把 Sleepy Doll 的控制面注入 BetterGI 进程，让原版 BetterGI 提供 `/bridge/v1` 接口。

不修改宿主源码、不写入宿主目录、不修改宿主配置；宿主进程退出后不残留任何改动。

## 目录

| 目录 | 内容 |
|---|---|
| `native/` | 注入器与引导 DLL。C++，只依赖 kernel32 |
| `managed/` | 桥本体。C#，运行于 BetterGI 进程内，无第三方依赖 |
| `recovery/` | 离线恢复工具，随发布一起分发 |
| `dev/` | 开发期专用：契约测试、元数据生成器、本地脚本 |
| `dist/` | 构建产物 |
| `.build/` | 四个 .NET 项目共用的中间目录，不进版本库 |

## 构建

```cmd
build.cmd
```

产物输出到 `dist/`。通常无需单独执行，根目录的 `build-desktop.cmd` 会调用本脚本。

## 配置

`bridge.config.json`（模板见 `bridge.config.example.json`）：

| 字段 | 作用 |
|---|---|
| `enabled` | 总开关。关掉后注入器不注入，已注入的实例对请求回 503 |
| `listen` | 监听地址，只允许回环 |
| `token` | 鉴权。留空时由 Sleepy Doll 首次连接时生成并写入 |
| `groups` | 按组开关。关掉的组不进 catalog，invoke 也拒绝 |
| `disabledMethods` | 单方法黑名单，优先级最高 |

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
