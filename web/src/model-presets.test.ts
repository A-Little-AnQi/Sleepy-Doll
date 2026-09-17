import { expect, it } from "vitest";
import { matchPreset, presetById, presetLabel } from "./model-presets";

it("does not pick a provider before the user does", () => {
  expect(presetById("")).toBeUndefined();
});

it("matches saved endpoints back to the preset", () => {
  expect(
    matchPreset({
      protocol: "anthropic-messages",
      baseUrl: "https://api.deepseek.com/anthropic/v1/",
    }),
  ).toBe("deepseek");
  expect(
    matchPreset({
      protocol: "openai-responses",
      baseUrl: "https://api.openai.com/v1",
    }),
  ).toBe("openai");
  expect(
    matchPreset({
      protocol: "openai-chat",
      baseUrl: "https://example.invalid/v1",
    }),
  ).toBe("custom");
});

it("labels a configured model with the preset name", () => {
  expect(
    presetLabel({
      protocol: "gemini",
      baseUrl: "https://generativelanguage.googleapis.com/v1beta",
    }),
  ).toBe("Gemini");
});
