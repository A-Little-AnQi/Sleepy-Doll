import type { CSSProperties, ReactNode } from "react";
import type { Page } from "../../web/src/App";
import type { Bootstrap } from "../../web/src/ipc/types";
import { AppShell } from "../../web/src/components/shell/AppShell";
import { TasksPage } from "../../web/src/pages/tasks/TasksPage";
import { bootstrapAt } from "../film";
import type { FilmSnapshot } from "../film";
import { FilmChatPane, type Presses } from "./FilmChatPane";

/**
 * 真实产品窗口：整套 AppShell（标题、侧栏、导航）直接用 fixture bootstrap
 * 驱动；内容区是真实 TasksPage 或复用 ChatPage 结构的 FilmChatPane。
 * 1440×900 设计稿等比缩放到舞台盒。
 */
export function ProductWindow({
  t,
  box,
  opacity,
  brandFade,
  blur,
  film,
  presses,
  viewport = { width: 1440, height: 900 },
  presentation,
  bootstrapOverride,
  planLayerStyle,
}: {
  t: number;
  box: { x: number; y: number; w: number; h: number };
  opacity: number;
  /** 品牌收束：0=正常，1=沉入夜色（透明 + 虚化 + 轻微后退）。 */
  brandFade: number;
  blur: number;
  film: FilmSnapshot;
  presses: Presses;
  viewport?: { width: number; height: number };
  presentation?: { page: Page; bootstrap: Bootstrap; content: ReactNode };
  /** 导演段等需要自定义 bootstrap 时覆盖，不传则按 t 取演示数据。 */
  bootstrapOverride?: Bootstrap;
  /** 导演段 C 镜计划行分层变量，透传给 FilmChatPane。 */
  planLayerStyle?: CSSProperties;
}) {
  const scale = box.w / viewport.width;
  const bootstrap =
    bootstrapOverride ?? presentation?.bootstrap ?? bootstrapAt(t);
  const noop = () => undefined;
  const windowOpacity = opacity * (1 - 0.66 * brandFade);
  const recede = 1 - 0.03 * brandFade;

  let content: ReactNode;
  if (presentation) {
    content = presentation.content;
  } else if (film.page === "tasks") {
    content = (
      <TasksPage
        bootstrap={bootstrap}
        reload={async () => undefined}
        onOpenConversation={noop}
        onOpenTask={noop}
        onConnectTools={noop}
      />
    );
  } else {
    content = (
      <FilmChatPane
        film={film}
        modelId={bootstrap.models[0]!.id}
        modelName={bootstrap.models[0]!.name}
        modelCode={bootstrap.models[0]!.model}
        contextWindow={bootstrap.models[0]!.contextWindow ?? 128000}
        permissionMode={bootstrap.permission.mode}
        permissionLevels={bootstrap.permission.levels.map((level) => ({
          value: level.value,
          label: level.label,
          description: level.description,
        }))}
        presses={presses}
        planLayerStyle={planLayerStyle}
      />
    );
  }

  return (
    <div
      className="video-window"
      style={{
        left: box.x,
        top: box.y,
        width: box.w,
        height: viewport.height * scale,
        opacity: windowOpacity,
        filter: blur > 0.05 ? `blur(${blur}px)` : undefined,
        transform: recede < 1 ? `scale(${recede})` : undefined,
        transformOrigin: "50% 42%",
      }}
    >
      <div
        style={{
          width: viewport.width,
          height: viewport.height,
          transform: `scale(${scale})`,
          transformOrigin: "0 0",
        }}
      >
        <AppShell
          bootstrap={bootstrap}
          page={presentation?.page ?? film.page}
          conversationId={presentation ? undefined : film.conversationId}
          detailsOpen={false}
          onPage={noop}
          onNew={noop}
          onConversation={noop}
          onToggleDetails={noop}
          reload={async () => undefined}
          composingNewChat={false}
        >
          {content}
        </AppShell>
      </div>
    </div>
  );
}
