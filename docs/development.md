# 开发环境

## 依赖

- Rust stable，edition 2024
- Node.js 22+
- Windows 10/11：WebView2 Runtime（通常随系统或 Edge 安装）
- Linux 桌面开发：Wry 所需的 GTK/WebKitGTK 开发包

## 构建与运行

```bash
npm ci
npm run check          # tsc --noEmit && vite build

cargo run --release    # 使用 <exe>/user/config.json
```

桌面打包前必须先执行 `npm run build`，因为 release 二进制会通过 `rust-embed` 编译进
`ui-dist/` 的静态资源。

可执行文件在 `target/<profile>/` 下，所以开发时的 `user/` 也落在那里；debug、release 与
mock 各自使用独立的 `user/` 目录。配置解析规则见 [配置与数据存放](./configuration.md)。

## 测试

```bash
cargo test --no-default-features --features mock   # 全部测试
cargo test --lib --no-default-features --features mock   # 仅单元测试
```

`--features mock` 是必需的：驱动 `AppController` 的测试套件都通过 Mock Backend 运行，而该
feature 不在默认集合里。CI 还会跑 `cargo fmt --all -- --check`、`cargo check` 和
`cargo check --no-default-features`。

## 离线 Mock Backend

Mock Backend 是独立 Rust 进程，模拟模型 API、BGI Bridge 和浏览器开发模式下的 IPC 网关。
它只监听 `127.0.0.1`，不访问外网，模型用量固定为 0。

它是**开发工具，不在发布版中**：由非默认的 `mock` feature 门控，默认构建与 release 二进制
既不编译 `src/mock.rs`，也不链接其 HTTP 依赖。它固定读取仓库根的
`sleepy-doll.mock.config.json`（编译期路径），不接受命令行参数，因此开发数据不会因为启动
位置不同而散落。

```bash
npm run mock   # 启动 mock 后端
npm run dev    # 启动 Vite 开发服务器
```

Mock 进程启动时会打印实际读取的配置路径与当前 `activeModel`。浏览器开发模式的 IPC 会把
模型请求转发给配置里的 `activeModel`，因此要完全离线，请让 `activeModel` 指向 Mock 模型。

浏览器开发模式会自动连接 `http://127.0.0.1:47124/ipc`；Wry 桌面模式仍使用原生 IPC，不受
影响。该端点绑定整个 `AppController`，所以它只接受 `http://localhost:5173`、
`http://127.0.0.1:5173`、`http://[::1]:5173` 和 `http://localhost:4173` 这几个开发来源
（精确匹配）；带其他 `Origin` 的浏览器请求会被 403 拒绝。

### 模拟的节奏

首个 Token 前约 220–510ms，流式输出按 1–5 个字符一块推进，块间延迟在约 50–110ms 内抖动；
句子标点处停顿更久，偶发 260–640ms 的上游停顿。可通过 `SLEEPY_DOLL_MOCK_MODEL_DELAY_MS`
和 `SLEEPY_DOLL_MOCK_CHUNK_DELAY_MS` 调整两个基准值，设为 `0` 可完全关闭对应延迟与抖动。

### 内置交流场景

- 包含“短回复”：返回单句，用于检查紧凑消息布局。
- 包含“长回复”：返回多段、编号和长行，用于检查换行、滚动、固定输入区与长会话标题。
- 包含“状态”或“连接”：模型调用 `bgi.state.get`，再根据 Mock Bridge 返回生成总结。
- 包含“路线”：模型调用 `bgi.capability.search`，再生成能力目录总结。
- Bridge 能力 `mock.success`、`mock.unknown`、`mock.failure`、`mock.busy`、`mock.denied`
  分别覆盖成功、结果未知、失败、忙碌和权限拒绝；Job 还支持运行中与取消。
