/**
 * 开发模式演示会话的标识。
 *
 * 单独一个模块：侧栏要在开发模式下渲染这条入口，而演示数据本身（`demo.ts`，近百 KB）
 * 只能在开发模式载入 —— 混在一起会把数据带进发布产物。
 */
export const DEMO_ID = "demo-conversation";
export const DEMO_TITLE = "模拟对话";
