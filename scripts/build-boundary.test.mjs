import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const root = fileURLToPath(new URL("../", import.meta.url));
const tool = (name) => name === "loom-frame" || name.startsWith("@loom-frame/");

test("normal npm installation has no local preview-tool dependency", () => {
  const manifest = JSON.parse(fs.readFileSync(path.join(root, "package.json")));
  for (const section of [
    "dependencies",
    "devDependencies",
    "optionalDependencies",
    "peerDependencies",
  ]) {
    for (const [name, version] of Object.entries(manifest[section] ?? {})) {
      assert.ok(
        !tool(name),
        `${section}.${name} would require optional tooling in CI`,
      );
      if (version.startsWith("file:")) {
        const location = path.resolve(root, version.slice(5));
        assert.ok(
          location.startsWith(root),
          `${name} depends on a file outside the checkout`,
        );
      }
    }
  }
});

test("the product TypeScript graph does not resolve preview adapters or tools", () => {
  const file = path.join(root, "tsconfig.json");
  const config = ts.readConfigFile(file, ts.sys.readFile);
  assert.equal(config.error, undefined);
  const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, root);
  assert.equal(parsed.errors.length, 0);
  const program = ts.createProgram(parsed.fileNames, parsed.options);
  const forbidden = program
    .getSourceFiles()
    .map((file) => path.resolve(file.fileName).replaceAll("\\", "/"))
    .filter(
      (file) =>
        file.startsWith(path.join(root, "frame").replaceAll("\\", "/") + "/") ||
        file.endsWith("/frame.config.ts") ||
        file.includes("/node_modules/loom-frame/") ||
        file.includes("/node_modules/@loom-frame/"),
    );
  assert.deepEqual(
    forbidden,
    [],
    "Product type checking pulled in optional preview tooling",
  );
});
