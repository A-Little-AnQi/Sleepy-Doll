import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { catalog } from "../frame/android-catalog.ts";
import { frameToolPackage } from "./frame-tooling.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const assets = path.join(root, "target/frame-android/assets");
const localProperties = path.join(root, "android/local.properties");
const properties = fs.existsSync(localProperties)
  ? fs.readFileSync(localProperties, "utf8")
  : "";
const bundledServer = path.join(
  frameToolPackage("@loom-frame/android"),
  "runtime/scrcpy-server",
);
const server =
  process.env.SLEEPY_SCRCPY_SERVER ??
  (fs.existsSync(bundledServer)
    ? bundledServer
    : properties.match(/^scrcpy.server=(.+)$/m)?.[1]?.trim());
if (!server || !fs.existsSync(server))
  throw new Error(
    "Install the bundled Android plugin or configure SLEEPY_SCRCPY_SERVER.",
  );
// This build matches the plugin's type-21 physical resize and geometry headers.
// Do not label an arbitrary scrcpy 4.1 binary as supporting those extensions.
const videoBuilds = new Map([
  [
    "dae105219de7c76cffdf5e76e8aca0e2406a57a8b4935db727b6767769be6d78",
    "loom-flex-3",
  ],
]);
const serverHash = createHash("sha256")
  .update(fs.readFileSync(server))
  .digest("hex");
const scrcpyBuild = videoBuilds.get(serverHash);
if (!scrcpyBuild)
  throw new Error(
    "Unverified Android preview video server: " +
      server +
      ". Use the matching @loom-frame/android bundled server; ordinary scrcpy does not provide the required geometry protocol.",
  );
const hostSource = fs.readFileSync(
  path.join(
    root,
    "android/preview-host/src/main/kotlin/dev/sleepydoll/preview/NativeComponents.kt",
  ),
  "utf8",
);
for (const entry of catalog)
  if (!hostSource.includes('"' + entry.id.split(":")[1] + '"'))
    throw new Error("Missing native component: " + entry.id);
fs.copyFileSync(server, path.join(assets, "scrcpy-server"));
const manifest = {
  protocol: 4,
  scrcpyVersion: "4.1",
  scrcpyBuild,
  package: "dev.sleepydoll.preview",
  activity: "dev.sleepydoll.preview/.PreviewActivity",
  componentIds: catalog.map((entry) => entry.id),
};
fs.writeFileSync(
  path.join(assets, "manifest.json"),
  JSON.stringify(manifest, null, 2) + "\n",
);
const sums =
  ["preview.apk", "scrcpy-server", "manifest.json"]
    .map(
      (name) =>
        createHash("sha256")
          .update(fs.readFileSync(path.join(assets, name)))
          .digest("hex") +
        "  " +
        name,
    )
    .join("\n") + "\n";
fs.writeFileSync(path.join(assets, "SHA256SUMS.txt"), sums);
console.log(
  JSON.stringify({
    assets,
    components: catalog.length,
    protocol: 4,
    scrcpyBuild,
    serverHash,
  }),
);
