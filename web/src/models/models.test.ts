import { expect, it } from "vitest";
import { resolveConversationModel } from ".";

const models = [
  { id: "alpha", active: false },
  { id: "beta", active: true },
];

it("uses the default model for a new conversation unless the user already picked one", () => {
  expect(resolveConversationModel(models)).toBe("beta");
  expect(resolveConversationModel(models, undefined, "alpha")).toBe("alpha");
});

it("keeps a conversation on its bound model", () => {
  expect(resolveConversationModel(models, { modelId: "alpha" })).toBe("alpha");
});

it("falls back to the default model when the bound one is gone", () => {
  expect(resolveConversationModel(models, { modelId: "deleted" })).toBe("beta");
  expect(resolveConversationModel(models, { modelId: null })).toBe("beta");
});

it("has no model when none are configured", () => {
  expect(resolveConversationModel([])).toBe("");
});
