import { expect, it } from "vitest";
import { MODEL_PRESETS, matchPreset, presetById, presetLabel } from "./presets";

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

it("keeps a developer platform route for every named provider", () => {
  for (const preset of MODEL_PRESETS.filter(
    (preset) => preset.id !== "custom",
  )) {
    expect(preset.platformUrl, preset.id).toMatch(/^https:\/\//);
    expect(preset.platformUrl, preset.id).not.toContain(" ");
  }
});
