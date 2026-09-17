import { expect, it } from "vitest";
import { HOST_PROVIDER, hostPluginEnabled, providerOfTool } from "./providers";

it("maps host tools to the host provider without exposing them in copy helpers", () => {
  expect(providerOfTool("bgi.state.get", "core:bgi")).toBe(HOST_PROVIDER);
  expect(providerOfTool("weather.http.get", "plugin:weather:http")).toBe(
    "weather",
  );
});

it("treats a missing host plugin row as introduced", () => {
  expect(hostPluginEnabled({ plugins: [] })).toBe(true);
  expect(
    hostPluginEnabled({
      plugins: [
        {
          manifest: { id: "bgi", name: "BetterGI", version: "1" },
          status: "disabled",
          configuredEnabled: false,
          host: true,
        },
      ],
    }),
  ).toBe(false);
});

