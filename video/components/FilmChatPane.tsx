import { useEffect, useRef, type CSSProperties } from "react";
import mascot from "../../web/src/brand/mascot.webp";
import { Transcript } from "../../web/src/components/chat/Transcript";
import { RunPlanCard } from "../../web/src/pages/chat/ChatPage";
import { ComposerDeck } from "../../web/src/components/chat/ComposerDeck";
import { ComposerField } from "../../web/src/components/chat/ComposerField";
import { Select } from "../../web/src/components/controls/Select";
import { ContextMeter } from "../../web/src/components/chat/ContextMeter";
import { CheckIcon, SendIcon } from "../../web/src/components/icons";
import { useT } from "../../web/src/i18n";
import { estimateMessagesTokens } from "../../web/src/session/context-usage";
import type { FilmSnapshot } from "../film";
import fixture from "../fixtures/launch-film.json";
import { ease, lerp } from "../clock";

export interface Presses {
  send?: boolean;
  allow?: boolean;
}

/**
 * 复用真实 ChatPage 的 DOM 结构与子组件（Transcript、RunPlanCard、
 * 审批卡、ComposerDeck/Field、Select、ContextMeter），状态全部来自 fixture。
 */
export function FilmChatPane({
  film,
  modelId,
  modelName,
  modelCode,
  contextWindow,
  permissionMode,
  permissionLevels,
  presses,
  planLayerStyle,
}: {
  film: FilmSnapshot;
  modelId: string;
  modelName: string;
  modelCode: string;
  contextWindow: number;
  permissionMode: string;
  permissionLevels: { value: string; label: string; description: string }[];
  presses: Presses;
  /** 导演段 C 镜：给真实计划行注入分层 CSS 变量；不传保持原样。 */
  planLayerStyle?: CSSProperties;
}) {
  const t = useT();
  const scroll = useRef<HTMLDivElement>(null);
  const welcome = film.welcome;
  const busy = !!film.phase || film.approvalVisible;
  const contextUsed = estimateMessagesTokens(film.messages, [""]);

  // 与产品一致：内容增长时跟随到底部。
  useEffect(() => {
    const el = scroll.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [film.messages, film.plan, film.approvalVisible, film.savedCardVisible]);

  const approvalEnter = ease.panel(film.approvalEnter);

  return (
    <section className={`chat-workspace${welcome ? " is-welcome" : ""}`}>
      <div className="chat-scene-switch">
        <div ref={scroll} className="chat-scroll">
          {welcome && (
            <div
              className="chat-welcome"
              style={
                film.welcomeFadeOut > 0
                  ? {
                      position: "absolute",
                      inset: "20px 24px 24px",
                      zIndex: 2,
                      margin: "auto",
                      opacity: 1 - film.welcomeFadeOut,
                    }
                  : undefined
              }
            >
              <img
                className="welcome-mascot"
                src={mascot}
                alt={t.chat.mascotAlt}
              />
              <h2>{t.chat.startNew}</h2>
            </div>
          )}
          {film.conversationId && (
            <div
              className="conversation-scene"
              style={{
                opacity: film.conversationFadeIn,
                transform: `translateY(${lerp(8, 0, ease.ui(film.conversationFadeIn))}px)`,
              }}
            >
              <div className="conversation-flow">
                <Transcript
                  key={film.messages.map((message) => `${message.role}:${message.content}`).join("|")}
                  messages={film.messages}
                  stream=""
                  phase={film.phase}
                  seconds={film.seconds}
                  toolLabels={film.toolLabels}
                />

                {film.plan && (
                  <div
                    className={planLayerStyle ? "director-plan-layer" : undefined}
                    style={{
                      opacity: ease.panel(film.planEnter),
                      transform: `translateY(${lerp(6, 0, ease.panel(film.planEnter)) - film.planShiftPx}px)`,
                      width: "100%",
                      display: "grid",
                      justifyItems: "start",
                      ...planLayerStyle,
                    }}
                  >
                    <RunPlanCard plan={film.plan} />
                  </div>
                )}

                {film.approvalVisible && (
                  <section
                    className="run-approval"
                    style={{
                      opacity: approvalEnter * (1 - film.approvalExit),
                      transform: `translateY(${lerp(10, 0, approvalEnter)}px)`,
                      marginBottom:
                        film.approvalExit > 0
                          ? `${-170 * ease.panel(film.approvalExit)}px`
                          : undefined,
                    }}
                  >
                    <h3>{t.chat.confirmExec}</h3>
                    <p>{fixture.approval.request.binding.description}</p>
                    <div className="detail-actions">
                      <button
                        type="button"
                        className="primary-action"
                        data-video-anchor="sd-allow"
                        data-video-press={presses.allow ? "1" : undefined}
                        tabIndex={-1}
                      >
                        允许
                      </button>
                      <button type="button" className="secondary-action" tabIndex={-1}>
                        拒绝
                      </button>
                    </div>
                  </section>
                )}

                {film.savedCardVisible > 0 && (
                  <div
                    style={{
                      opacity: film.savedCardVisible,
                      transform: `scale(${lerp(0.94, 1, ease.panel(film.savedCardVisible))})`,
                      transformOrigin: "top center",
                      width: "min(100%, 440px)",
                    }}
                  >
                    <div className="film-saved-card">
                      <CheckIcon className="button-icon" />
                      <div>
                        <strong>已保存「{fixture.workflow.name}」</strong>
                        <span>{fixture.workflow.description}</span>
                      </div>
                    </div>
                  </div>
                )}
              </div>
            </div>
          )}
        </div>
      </div>
      <div className="composer-dock">
        <ComposerDeck>
          <ComposerField
            aria-label={t.chat.message}
            placeholder={
              busy ? t.chat.composerPlaceholderBusy : t.chat.composerPlaceholderNew
            }
            value={film.composerDraft}
            tabIndex={-1}
            onChange={() => undefined}
          />
          <div className="composer-actions">
            <div className="composer-menu composer-approval">
              <Select
                label={t.chat.approvalLevel}
                value={permissionMode}
                options={permissionLevels.map((level) => ({
                  value: level.value,
                  label: level.label,
                  description: level.description,
                }))}
                onChange={() => undefined}
              />
            </div>
            <div className="composer-submit">
              <ContextMeter used={contextUsed} window={contextWindow} />
              <div className="composer-menu composer-model">
                <Select
                  label={t.chat.model}
                  value={modelId}
                  options={[
                    { value: modelId, label: modelName, description: modelCode },
                  ]}
                  onChange={() => undefined}
                />
              </div>
              <button
                type="button"
                className="send-action"
                aria-label={t.chat.send}
                title={t.chat.send}
                disabled={!film.composerDraft.trim()}
                data-video-anchor="sd-send"
                data-video-press={presses.send ? "1" : undefined}
                tabIndex={-1}
              >
                <SendIcon className="button-icon" />
              </button>
            </div>
          </div>
        </ComposerDeck>
      </div>
    </section>
  );
}
