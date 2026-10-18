// Requires a running Frame Android preview and ffmpeg on PATH.
// Captures stay inside target/tmp/android/resize for inspection and scoped cleanup.
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import {
  frameToolRequire,
  frameToolFile,
} from "../../scripts/frame-tooling.mjs";
import { readAndroidDesign } from "../../frame/android-design.ts";
frameToolFile("@loom-frame/android", "dist/index.mjs");
const { WebSocket } = frameToolRequire("ws");

const root = fileURLToPath(new URL("../../", import.meta.url));
const output = path.join(root, "target/tmp/android/resize");
fs.mkdirSync(output, { recursive: true });
const base = process.env.FRAME_URL ?? "http://127.0.0.1:4321";
const status = await (
  await fetch(base + "/runtime/android/api/android")
).json();
assert.equal(status.phase, "ready", "Enable the Android preview first");
const health = await (await fetch(base + "/runtime/android/api/health")).json();
const origin = "http://127.0.0.1:" + health.port;
const socket = new WebSocket(
  origin.replace("http", "ws") + "/api/android/stream",
  { origin },
);
const events = [];
const packets = [];
socket.on("message", (data, binary) => {
  if (binary) {
    const packet = Buffer.from(data);
    packets.push(packet.subarray(12));
  } else events.push(JSON.parse(data.toString()));
});
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function wait(predicate, label) {
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    const match = events.find(predicate);
    if (match) return match;
    const failure = events.find((event) => event.type === "error");
    if (failure) throw new Error(JSON.stringify(failure));
    await sleep(20);
  }
  throw new Error("Timeout: " + label);
}
const send = (value) => socket.send(JSON.stringify(value));
let revision = "resize-initial";
function render(width, height, pixelRatio = 1) {
  send({
    type: "render",
    id: "sleepy-doll-android:button",
    props: { label: "Resize check", clicks: 0 },
    theme: readAndroidDesign().light,
    revision,
    playback: 0,
    width,
    height,
    pixelRatio,
  });
}
async function click(x, y) {
  send({ type: "touch", action: 0, x, y });
  await sleep(80);
  send({ type: "touch", action: 1, x, y });
}
const results = [];
try {
  await new Promise((resolve, reject) => {
    socket.once("open", resolve);
    socket.once("error", reject);
  });
  send({ type: "hello", codecs: ["h264"] });
  render(280, 372);
  await wait(
    (event) => event.type === "still" && event.revision === revision,
    "initial frame",
  );

  // Size-only updates deliberately retain the content revision, like the Worker.
  const cases = [
    [324, 432, 1],
    [480, 320, 1],
    [280, 600, 1],
    [500, 900, 2],
    [360, 480, 1],
  ];
  for (const [index, [width, height, ratio]] of cases.entries()) {
    const start = events.length;
    for (let frame = 1; frame <= 12; frame++) {
      const fromWidth = index ? cases[index - 1][0] : 280;
      const fromHeight = index ? cases[index - 1][1] : 372;
      render(
        Math.round(fromWidth + ((width - fromWidth) * frame) / 12),
        Math.round(fromHeight + ((height - fromHeight) * frame) / 12),
        ratio,
      );
      await sleep(33);
    }
    const physical = [
      Math.floor((width * ratio) / 2) * 2,
      Math.floor((height * ratio) / 2) * 2,
    ];
    const size = await wait(
      (event) =>
        events.indexOf(event) >= start &&
        event.type === "size" &&
        event.renderWidth === physical[0] &&
        event.renderHeight === physical[1],
      "physical resize " + physical.join("x"),
    );
    // The flex build keeps a stable encoder surface during a drag; its geometry
    // header and Android's real display must agree on the new render dimensions.
    assert.ok(size.width > 0 && size.height > 0);
    await sleep(350);
    const displays = spawnSync(
      frameToolFile(
        "@loom-frame/android",
        "runtime/sdk/platform-tools/adb.exe",
      ),
      [
        "-P",
        process.env.FRAME_ADB_PORT ?? "5038",
        "-s",
        status.serial,
        "shell",
        "dumpsys",
        "display",
      ],
      {
        encoding: "utf8",
        windowsHide: true,
        env: { ...process.env, TEMP: output, TMP: output, TMPDIR: output },
      },
    );
    assert.equal(displays.status, 0, displays.stderr);
    assert.ok(
      displays.stdout
        .split(/\r?\n/)
        .some(
          (line) =>
            line.includes('mBaseDisplayInfo=DisplayInfo{"scrcpy"') &&
            line.includes("real " + physical.join(" x ") + ","),
        ),
      "Android physical display did not resize: " + physical.join("x"),
    );
    revision = "resize-click-" + index;
    render(width, height, ratio);
    await wait(
      (event) => event.type === "painted" && event.revision === revision,
      "paint after resize",
    );
    await wait(
      (event) =>
        event.type === "still" &&
        event.revision === revision &&
        event.width === physical[0] &&
        event.height === physical[1],
      "window after resize",
    );
    send({ type: "keyframe" });
    await sleep(400);
    const stem = path.join(
      output,
      "size-" + width + "x" + height + "-dpr" + ratio,
    );
    fs.writeFileSync(stem + ".h264", Buffer.concat(packets), { flag: "wx" });
    assert.ok(!fs.existsSync(stem + ".png"), "Capture already exists");
    const decoded = spawnSync(
      "ffmpeg",
      [
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-f",
        "h264",
        "-i",
        stem + ".h264",
        "-vf",
        "scale=" + physical.join(":"),
        "-update",
        "1",
        stem + ".png",
      ],
      {
        windowsHide: true,
        env: { ...process.env, TEMP: output, TMP: output, TMPDIR: output },
      },
    );
    assert.equal(decoded.status, 0, decoded.stderr?.toString());
    assert.ok(fs.existsSync(stem + ".png"), "No video frames decoded");
    const corners = spawnSync(
      "ffmpeg",
      [
        "-hide_banner",
        "-loglevel",
        "error",
        "-i",
        stem + ".png",
        "-filter_complex",
        "split=4[a][b][c][d];[a]crop=2:2:1:1[aa];[b]crop=2:2:iw-3:1[bb];[c]crop=2:2:1:ih-3[cc];[d]crop=2:2:iw-3:ih-3[dd];[aa][bb][cc][dd]hstack=inputs=4",
        "-pix_fmt",
        "rgb24",
        "-f",
        "rawvideo",
        "pipe:1",
      ],
      {
        windowsHide: true,
        env: { ...process.env, TEMP: output, TMP: output, TMPDIR: output },
      },
    );
    assert.equal(corners.status, 0, corners.stderr?.toString());
    assert.equal(corners.stdout.length, 48);
    const background = readAndroidDesign()
      .light.surface.slice(1)
      .match(/../g)
      .map((channel) => parseInt(channel, 16));
    assert.ok(
      [...corners.stdout].every(
        (value, channel) => Math.abs(value - background[channel % 3]) < 18,
      ),
      "Video corner contains a black border",
    );

    // New props must remain editable at the resized coordinate system's centre.
    await click(size.width / 2, size.height / 2);
    assert.equal(
      (
        await wait(
          (event) =>
            event.type === "patch" &&
            event.revision === revision &&
            event.key === "clicks",
          "touch after resize",
        )
      ).value,
      1,
    );
    results.push({
      css: [width, height],
      ratio,
      physical,
      encoded: [size.width, size.height],
      touch: true,
    });
    console.log(JSON.stringify(results.at(-1)));
  }
  fs.writeFileSync(
    path.join(output, "results.json"),
    JSON.stringify(results, null, 2),
    { flag: "wx" },
  );
} finally {
  socket.close();
  await new Promise((resolve) => {
    socket.once("close", resolve);
    setTimeout(() => {
      socket.terminate();
      resolve();
    }, 1000).unref();
  });
}
