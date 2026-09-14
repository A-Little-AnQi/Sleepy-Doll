import { MOCK_BACKEND, api } from "./api";
import { DEMO_PROMPT } from "./demo";
import { DEMO_ID } from "./demo-id";

/**
 * 开发模式的「模拟对话」。
 *
 * 这里不做演出 —— 只替你发出录制的开场白：mock 后端回放录制的助手轮次，工具真实
 * 执行，事件流照常推送，界面按正常路径渲染。后端或链路出问题在演示里就能看见，
 * 一段前端动画会把问题盖过去。
 */
const RESET = new URL("/dev/reset-demo", MOCK_BACKEND).toString();

/** 每次页面加载换一个会话 id，演示之间互不干扰。 */
function conversationId() {
  const key = "sleepy-doll-demo-conversation";
  let id = sessionStorage.getItem(key);
  if (!id) {
    id = `${DEMO_ID}-${crypto.randomUUID().slice(0, 8)}`;
    sessionStorage.setItem(key, id);
  }
  return id;
}

export const demoConversationId = conversationId;

let pending = false;

/** 点「模拟对话」时调用：清掉上一次的痕迹，再发出开场白。 */
export async function startDemo() {
  if (pending) return;
  pending = true;
  const id = conversationId();
  try {
    // 重置只由 mock 提供。失败不阻塞演示 —— 最坏是侧栏多留一条旧会话。
    await fetch(RESET, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ conversationId: id }),
    });
    await api.submitTask(DEMO_PROMPT, id, id);
  } catch {
    pending = false;
  }
}
