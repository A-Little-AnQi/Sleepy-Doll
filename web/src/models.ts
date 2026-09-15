import type { ModelInfo } from "./types";

export function defaultModelId(
  models: ReadonlyArray<Pick<ModelInfo, "id" | "active">>,
): string {
  return models.find((model) => model.active)?.id ?? models[0]?.id ?? "";
}

/** 新对话用默认模型；已有对话用自己绑的那一个。绑的已经被删了就回落到默认。 */
export function resolveConversationModel(
  models: ReadonlyArray<Pick<ModelInfo, "id" | "active">>,
  conversation?: { modelId?: string | null } | null,
  pending?: string | null,
): string {
  const fallback = defaultModelId(models);
  const known = new Set(models.map((model) => model.id));
  if (!conversation) {
    return pending && known.has(pending) ? pending : fallback;
  }
  if (conversation.modelId && known.has(conversation.modelId)) {
    return conversation.modelId;
  }
  return fallback;
}
