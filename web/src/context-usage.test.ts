import { expect, it } from "vitest";
import {
  estimateMessagesTokens,
  estimateTokens,
  formatTokens,
} from "./context-usage";

it("matches the runtime character-based token estimate", () => {
  expect(estimateTokens("中文字符")).toBe(4);
  expect(estimateTokens("abcdefgh")).toBe(2);
  expect(estimateMessagesTokens([{ content: "中".repeat(10) }])).toBe(18);
});

it("formats context usage for the composer meter", () => {
  expect(formatTokens(0)).toBe("0");
  expect(formatTokens(870)).toBe("870");
  expect(formatTokens(12_400)).toBe("12k");
  expect(formatTokens(200_000)).toBe("200k");
});
