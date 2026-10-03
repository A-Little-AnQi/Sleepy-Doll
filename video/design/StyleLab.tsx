import { useEffect, useMemo, useRef, useState } from "react";
import * as THREE from "three";
import { useClock, clamp01 } from "../clock";
import type { NativeState } from "./native-scenes";
import "./style-lab.css";

/**
 * 风格试验台：5 个 8 秒镜头（共 40 秒），全部用当前源码真实组件截图做纹理，
 * 用 three 实现几何 / SDF / 网格形变 / 折页 / 折射五种互不重复的运动逻辑。
 * 所有状态是绝对时间 t 的纯函数：preset = floor(t/8000)，local = t%8000，
 * 录制 seek 可精确重定位；wallclock 只用于预览播放（由外层时钟驱动）。
 */

export const STYLE_SCENE_MS = 8000;
export const STYLE_SCENES = 5;
export const STYLE_DURATION_MS = STYLE_SCENE_MS * STYLE_SCENES;
export const STYLE_SPEC = {
  width: 1920,
  height: 1080,
  fps: 60,
  durationMs: STYLE_DURATION_MS,
} as const;

export interface StyleRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface StyleStateRects {
  send?: StyleRect;
  composer?: StyleRect;
  userMessage?: StyleRect;
  approval?: StyleRect;
  approvalAllow?: StyleRect;
  plan?: StyleRect;
  planTight?: StyleRect;
  planRows?: StyleRect[];
  planRowsTight?: StyleRect[];
  taskButton?: StyleRect;
  receipt?: StyleRect;
}

export interface StyleInputs {
  images: Record<NativeState, string>;
  rects: Record<NativeState, StyleStateRects>;
}

const FONT_STACK = '"Microsoft YaHei", "PingFang SC", "Noto Sans SC", sans-serif';
const BG_BLACK = "#090A0C";
const NATIVE_W = 1440;
const NATIVE_H = 900;
const LOAD_STATES: NativeState[] = [
  "compose",
  "message",
  "plan",
  "approval",
  "run",
  "submitted",
];

declare global {
  interface Window {
    __STYLE_INPUTS__?: StyleInputs;
    __STYLE_READY__?: boolean;
  }
}

const seg = (t: number, a: number, b: number) => clamp01((t - a) / (b - a));
const easeOut = (k: number) => 1 - Math.pow(1 - k, 3);
const easeInOut = (k: number) =>
  k < 0.5 ? 4 * k * k * k : 1 - Math.pow(-2 * k + 2, 3) / 2;

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error(`图片载入失败：${src.slice(0, 48)}`));
    img.src = src;
  });
}

/** 把已载入图片按归一化 rect 裁到新 canvas，按 naturalWidth/Height 取样（高清源）。 */
function cropCanvas(img: HTMLImageElement, rect?: StyleRect): HTMLCanvasElement {
  if (!rect) {
    const c = document.createElement("canvas");
    c.width = img.naturalWidth;
    c.height = img.naturalHeight;
    c.getContext("2d")!.drawImage(img, 0, 0);
    return c;
  }
  const sx = rect.x * img.naturalWidth;
  const sy = rect.y * img.naturalHeight;
  const sw = rect.w * img.naturalWidth;
  const sh = rect.h * img.naturalHeight;
  const c = document.createElement("canvas");
  c.width = Math.max(2, Math.round(sw));
  c.height = Math.max(2, Math.round(sh));
  c.getContext("2d")!.drawImage(img, sx, sy, sw, sh, 0, 0, c.width, c.height);
  return c;
}

/** 白底转透明、深色墨迹转白字 alpha：只搬运真实文字像素，不重打字。 */
function inkToWhite(canvas: HTMLCanvasElement): HTMLCanvasElement {
  const ctx = canvas.getContext("2d", { willReadFrequently: true })!;
  const data = ctx.getImageData(0, 0, canvas.width, canvas.height);
  const px = data.data;
  for (let i = 0; i < px.length; i += 4) {
    const lum = (px[i]! * 0.299 + px[i + 1]! * 0.587 + px[i + 2]! * 0.114) / 255;
    px[i + 3] = Math.round(Math.max(0, Math.min(1, 1 - lum)) * px[i + 3]!);
    px[i] = 255;
    px[i + 1] = 255;
    px[i + 2] = 255;
  }
  ctx.putImageData(data, 0, 0);
  return canvas;
}

function canvasTexture(c: HTMLCanvasElement): THREE.Texture {
  const tex = new THREE.CanvasTexture(c);
  tex.colorSpace = THREE.SRGBColorSpace;
  tex.minFilter = THREE.LinearFilter;
  tex.generateMipmaps = false;
  return tex;
}

function textCanvas(
  text: string,
  size: number,
  color: string,
  weight = "700",
): { canvas: HTMLCanvasElement; width: number } {
  const c = document.createElement("canvas");
  const ctx = c.getContext("2d")!;
  ctx.font = `${weight} ${size}px ${FONT_STACK}`;
  const metrics = ctx.measureText(text);
  c.width = Math.ceil(metrics.width) + 40;
  c.height = Math.ceil(size * 1.5);
  const ctx2 = c.getContext("2d")!;
  ctx2.font = `${weight} ${size}px ${FONT_STACK}`;
  ctx2.fillStyle = color;
  ctx2.textAlign = "center";
  ctx2.textBaseline = "middle";
  ctx2.fillText(text, c.width / 2, c.height / 2);
  return { canvas: c, width: c.width };
}

/**
 * 真欧氏距离变换（Felzenszwalb 1D 平方距离两遍：先行后列），
 * 返回有符号距离（像素），字形内部为负、外部为正。
 * 阈值读亮度（黑底白字），不是 alpha。
 */
function signedDistanceField(
  text: string,
  size = 220,
): { data: Float32Array; width: number; height: number } {
  const N = 512;
  const c = document.createElement("canvas");
  c.width = c.height = N;
  const ctx = c.getContext("2d", { willReadFrequently: true })!;
  ctx.fillStyle = "#000";
  ctx.fillRect(0, 0, N, N);
  ctx.font = `700 ${size}px ${FONT_STACK}`;
  ctx.fillStyle = "#fff";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText(text, N / 2, N / 2 + size * 0.04);
  const raw = ctx.getImageData(0, 0, N, N).data;
  const glyph = new Uint8Array(N * N);
  let glyphCount = 0;
  for (let i = 0; i < glyph.length; i += 1) {
    glyph[i] = raw[i * 4]! > 127 ? 1 : 0;
    glyphCount += glyph[i]!;
  }
  const coverage = glyphCount / glyph.length;
  if (coverage < 0.02 || coverage > 0.6)
    throw new Error(`SDF 字形覆盖异常：${text} coverage=${coverage.toFixed(3)}`);

  const INF = 1e12;
  // 每行/列的 1D 平方 EDT。
  const edt1d = (f: Float64Array, n: number) => {
    const d = new Float64Array(n);
    const v = new Int32Array(n);
    const z = new Float64Array(n + 1);
    let k = 0;
    v[0] = 0;
    z[0] = -INF;
    z[1] = INF;
    for (let q = 1; q < n; q += 1) {
      const sep = (kAt: number) => {
        const vk = v[kAt] ?? 0;
        return ((f[q] ?? 0) + q * q - ((f[vk] ?? 0) + vk * vk)) / (2 * q - 2 * vk);
      };
      let s = sep(k);
      while (s <= (z[k] ?? -INF)) {
        k -= 1;
        s = sep(k);
      }
      k += 1;
      v[k] = q;
      z[k] = s;
      z[k + 1] = INF;
    }
    k = 0;
    for (let q = 0; q < n; q += 1) {
      while (z[k + 1]! < q) k += 1;
      d[q] = (q - v[k]!) * (q - v[k]!) + f[v[k]!]!;
    }
    return d;
  };
  const edt2d = (inside: boolean) => {
    const g = new Float64Array(N * N);
    for (let i = 0; i < g.length; i += 1)
      g[i] = (glyph[i] === 1) === inside ? 0 : INF;
    const row = new Float64Array(N);
    const col = new Float64Array(N);
    for (let y = 0; y < N; y += 1) {
      for (let x = 0; x < N; x += 1) row[x] = g[y * N + x]!;
      const d = edt1d(row, N);
      for (let x = 0; x < N; x += 1) g[y * N + x] = d[x]!;
    }
    for (let x = 0; x < N; x += 1) {
      for (let y = 0; y < N; y += 1) col[y] = g[y * N + x]!;
      const d = edt1d(col, N);
      for (let y = 0; y < N; y += 1) g[y * N + x] = d[y]!;
    }
    return g;
  };
  // inside = 白色字形：din = 到字形的距离（mask 内 0），dout = 到背景的距离。
  const din = edt2d(true);
  const dout = edt2d(false);
  const out = new Float32Array(N * N);
  let min = Infinity;
  let max = -Infinity;
  for (let i = 0; i < out.length; i += 1) {
    out[i] = Math.sqrt(din[i]!) - Math.sqrt(dout[i]!);
    if (!Number.isFinite(out[i]!)) throw new Error("SDF 出现非有限值");
    min = Math.min(min, out[i]!);
    max = Math.max(max, out[i]!);
  }
  if (min >= 0 || max <= 0)
    throw new Error(`SDF 符号异常：min=${min} max=${max}`);
  // 逐 mask 采样断言：mask 内 distance<0、mask 外 distance>0。
  for (let i = 0; i < out.length; i += 1) {
    const d = out[i]!;
    if (glyph[i] === 1 ? d >= 0 : d <= 0)
      throw new Error(
        `SDF mask 符号断言失败：像素 ${i}（x=${i % N},y=${Math.floor(i / N)}）在 ${glyph[i] === 1 ? "字形内" : "字形外"} 但 distance=${d}`,
      );
  }
  return { data: out, width: N, height: N };
}

function sdfTexture(sdf: { data: Float32Array; width: number; height: number }): THREE.DataTexture {
  const tex = new THREE.DataTexture(
    sdf.data,
    sdf.width,
    sdf.height,
    THREE.RedFormat,
    THREE.FloatType,
  );
  tex.minFilter = THREE.LinearFilter;
  tex.magFilter = THREE.LinearFilter;
  tex.wrapS = THREE.ClampToEdgeWrapping;
  tex.wrapT = THREE.ClampToEdgeWrapping;
  tex.needsUpdate = true;
  return tex;
}

interface SceneKit {
  scene: THREE.Scene;
  camera: THREE.Camera;
  render(local: number): void;
  dispose(): void;
}

function orthoCamera(): THREE.OrthographicCamera {
  return new THREE.OrthographicCamera(-960, 960, 540, -540, -4000, 4000);
}

/** 帧时间轴里按区间取激活值（毫秒，local 0..8000）。 */
function opacityWindow(local: number, a: number, b: number, fade = 200): number {
  if (local < a || local > b) return 0;
  return Math.min(seg(local, a, a + fade), 1 - seg(local, b - fade, b));
}

/* ------------------------------------------------------------------ */
/* 镜头 01：黑白文字曲面 —— 大字贴细分曲面逐字卷曲，真实 UI 占中下。 */
function buildScene01(imgs: Record<NativeState, HTMLImageElement>, rects: StyleInputs["rects"]): SceneKit {
  const scene = new THREE.Scene();
  scene.background = new THREE.Color(BG_BLACK);
  const camera = new THREE.PerspectiveCamera(38, 1920 / 1080, 10, 8000);
  camera.position.set(0, 0, 1500);

  const big = textCanvas("先确认，再执行。", 128, "#FFFFFF");
  const bigTex = canvasTexture(big.canvas);
  const W = 1560;
  const H = (W * big.canvas.height) / big.canvas.width;
  const geo = new THREE.PlaneGeometry(W, H, 96, 24);
  // 局部折回幅度限制：峰值 θ = (W/2)·A ≤ 0.8 rad，字始终可读。
  const FOLD_A = 1.6 / W;
  const material = new THREE.ShaderMaterial({
    transparent: true,
    depthWrite: false,
    side: THREE.DoubleSide,
    uniforms: {
      uMap: { value: bigTex },
      uK: { value: 0 },
      uFoldA: { value: 0 },
      uFoldX: { value: 0 },
      uFoldW: { value: 180 },
    },
    vertexShader: /* glsl */ `
      uniform float uK; uniform float uFoldA; uniform float uFoldX; uniform float uFoldW;
      varying vec2 vUv; varying float vShade;
      float curvature(float x) {
        return uK + uFoldA * exp(-pow(x - uFoldX, 2.0) / (uFoldW * uFoldW));
      }
      vec2 curl(float x) {
        float k = curvature(x);
        float th = x * k;
        if (abs(k) < 1e-7) return vec2(x, 0.0);
        return vec2(sin(th) / k, (1.0 - cos(th)) / k);
      }
      void main() {
        vUv = uv;
        float dx = ${((W / 96) * 1.5).toFixed(2)};
        vec2 p0 = curl(position.x);
        vec2 pm = curl(position.x - dx);
        vec2 pp = curl(position.x + dx);
        vec3 tangent = normalize(vec3(pp.x - pm.x, 0.0, pp.y - pm.y));
        vec3 n = normalize(cross(tangent, vec3(0.0, 1.0, 0.0)));
        if (n.z < 0.0) n = -n;
        vShade = 0.55 + 0.45 * max(dot(n, normalize(vec3(0.3, 0.55, 0.78))), 0.0);
        gl_Position = projectionMatrix * modelViewMatrix * vec4(p0.x, position.y, p0.y, 1.0);
      }
    `,
    fragmentShader: /* glsl */ `
      uniform sampler2D uMap;
      varying vec2 vUv; varying float vShade;
      void main() {
        vec4 t = texture2D(uMap, vUv);
        if (t.a < 0.02) discard;
        gl_FragColor = vec4(vec3(1.0) * vShade, t.a);
      }
    `,
  });
  const textMesh = new THREE.Mesh(geo, material);
  textMesh.position.set(0, 130, 0);
  scene.add(textMesh);

  // 真实 UI 证据：宽固定 ≤1200，高按裁剪 aspect，不整窗低透明度糊成一坨。
  const mkPlane = (canvas: HTMLCanvasElement, width: number) => {
    const h = (width * canvas.height) / canvas.width;
    const mesh = new THREE.Mesh(
      new THREE.PlaneGeometry(width, h),
      new THREE.MeshBasicMaterial({ map: canvasTexture(canvas), transparent: true, depthWrite: false }),
    );
    scene.add(mesh);
    return { mesh, height: h };
  };
  const composePlane = mkPlane(cropCanvas(imgs.compose, rects.compose.composer), 900);
  composePlane.mesh.position.set(0, -240, -40);
  const approvalCrop = cropCanvas(imgs.approval, rects.approval.approval);
  const approvalPlane = mkPlane(approvalCrop, 1180);
  approvalPlane.mesh.position.set(0, -210, -20);
  const messagePlane = mkPlane(cropCanvas(imgs.submitted, rects.submitted.receipt), 1040);
  messagePlane.mesh.position.set(0, -220, -40);

  return {
    scene,
    camera,
    render(local) {
      // 0–2s 从卷曲展开；2–4s 受控局部折回（θ≤0.8rad）；4–6s 落平；6.8s 后完全落定。
      const unroll = 1 - easeInOut(seg(local, 0, 2000));
      material.uniforms.uK!.value = (1 / 520) * unroll;
      const fold = Math.sin(Math.PI * seg(local, 2000, 4000));
      material.uniforms.uFoldA!.value = FOLD_A * fold;
      material.uniforms.uFoldX!.value = -W / 2 + W * seg(local, 2000, 4000);
      composePlane.mesh.material.opacity = opacityWindow(local, 150, 2150);
      approvalPlane.mesh.material.opacity = opacityWindow(local, 2300, 4150, 250);
      messagePlane.mesh.material.opacity = opacityWindow(local, 4400, 8000, 250);
    },
    dispose() {
      geo.dispose();
      material.dispose();
      scene.traverse((o) => {
        const m = (o as THREE.Mesh).material as THREE.Material | undefined;
        m?.dispose();
      });
    },
  };
}

/* ------------------------------------------------------------------ */
/* 镜头 02：白场字形轮廓 —— 真 SDF 插值 + 边界涡旋变形。 */
function buildScene02(imgs: Record<NativeState, HTMLImageElement>, rects: StyleInputs["rects"]): SceneKit {
  const scene = new THREE.Scene();
  scene.background = new THREE.Color("#FFFFFF");
  const camera = orthoCamera();
  camera.position.z = 100;

  const sdfA = signedDistanceField("目标");
  const sdfB = signedDistanceField("确认");
  const sdfC = signedDistanceField("执行");
  const texA = sdfTexture(sdfA);
  const texB = sdfTexture(sdfB);
  const texC = sdfTexture(sdfC);
  // DataTexture flipY=false：数据行 0 是顶行，shader 采样时翻转 v。
  const material = new THREE.ShaderMaterial({
    uniforms: {
      uA: { value: texA },
      uB: { value: texB },
      uC: { value: texC },
      uMix1: { value: 0 },
      uMix2: { value: 0 },
      uAccent: { value: 0 },
    },
    vertexShader: /* glsl */ `
      varying vec2 vUv;
      void main() { vUv = uv; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0); }
    `,
    fragmentShader: /* glsl */ `
      uniform sampler2D uA; uniform sampler2D uB; uniform sampler2D uC;
      uniform float uMix1; uniform float uMix2; uniform float uAccent;
      varying vec2 vUv;
      // 字形占屏幕中部 58%；SDF 单位是像素，512 图幅。
      float dAt(sampler2D s, vec2 uv) {
        vec2 g = (uv - vec2(0.5, 0.52)) / vec2(0.58 * 1080.0 / 1920.0, 0.58) + 0.5;
        if (g.x < 0.0 || g.x > 1.0 || g.y < 0.0 || g.y > 1.0) return 200.0;
        return texture2D(s, vec2(g.x, 1.0 - g.y)).r;
      }
      float field(vec2 uv) {
        float d1 = mix(dAt(uA, uv), dAt(uB, uv), uMix1);
        return mix(d1, dAt(uC, uv), uMix2);
      }
      void main() {
        vec2 uv = vUv;
        // 只作用在字形边界附近的连续涡旋：混合中段最强，静止帧为 0。
        float phase = uMix1 * (1.0 - uMix1) + uMix2 * (1.0 - uMix2);
        vec2 c = vec2(0.5, 0.52);
        vec2 rel = uv - c;
        float sw = phase * 0.028 * exp(-pow(length(rel) / 0.36, 2.0));
        vec2 distorted = uv + vec2(-rel.y, rel.x) * sw;
        float d = field(distorted);
        float px = 1.6;
        float glyph = 1.0 - smoothstep(-px, px, d);
        vec3 col = mix(vec3(1.0), vec3(0.035, 0.04, 0.05), glyph);
        // 少量冷红：字形边界上的当前信号。
        float edge = exp(-pow(d / 14.0, 2.0)) * uAccent;
        col = mix(col, vec3(0.78, 0.16, 0.22), clamp(edge, 0.0, 1.0) * 0.85);
        gl_FragColor = vec4(col, 1.0);
      }
    `,
  });
  scene.add(new THREE.Mesh(new THREE.PlaneGeometry(1920, 1080), material));

  const kit: SceneKit = {
    scene,
    camera,
    render(local) {
      material.uniforms.uMix1!.value = easeInOut(seg(local, 2600, 4100));
      material.uniforms.uMix2!.value = easeInOut(seg(local, 4100, 5000));
      // uAccent 恒为 0：白底黑字，无红色霓虹描边。
    },
    dispose() {
      material.dispose();
      [texA, texB, texC].forEach((t) => t.dispose());
    },
  };
  void imgs;
  void rects;
  return kit;
}

/* ------------------------------------------------------------------ */
/* 镜头 03：冷蓝界面弹性 —— 真实 composer 近景 64×48 网格 z 波形变。 */
function buildScene03(imgs: Record<NativeState, HTMLImageElement>, rects: StyleInputs["rects"]): SceneKit {
  const scene = new THREE.Scene();
  scene.background = new THREE.Color("#F4F8FE");
  const camera = new THREE.PerspectiveCamera(36, 1920 / 1080, 10, 8000);
  camera.position.set(0, 60, 1440);
  camera.lookAt(0, -40, 0);

  const composer = rects.compose.composer ?? { x: 0.1, y: 0.75, w: 0.8, h: 0.2 };
  const send = rects.compose.send ?? { x: 0.9, y: 0.9, w: 0.05, h: 0.05 };
  const composerCanvas = cropCanvas(imgs.compose, composer);
  const W = 1440;
  const H = (W * composerCanvas.height) / composerCanvas.width;
  // send 中心相对 composer 裁剪区换算。
  const originU = clamp01((send.x + send.w / 2 - composer.x) / composer.w);
  const originV = clamp01(1 - (send.y + send.h / 2 - composer.y) / composer.h);
  const texC = canvasTexture(composerCanvas);
  const geo = new THREE.PlaneGeometry(W, H, 64, 48);
  const AMP = 80;
  const material = new THREE.ShaderMaterial({
    uniforms: {
      uMap: { value: texC },
      uOrigin: { value: new THREE.Vector2(originU, originV) },
      uWave: { value: 0 },
      uPhase: { value: 0 },
    },
    vertexShader: /* glsl */ `
      uniform vec2 uOrigin; uniform float uWave; uniform float uPhase;
      varying vec2 vUv; varying float vShade;
      float bump(vec2 uv) {
        vec2 d = (uv - uOrigin) * vec2(${W.toFixed(1)}, ${H.toFixed(1)});
        float r = length(d);
        float sigma = 260.0;
        return uWave * exp(-r * r / (sigma * sigma)) * sin(0.024 * r - uPhase);
      }
      void main() {
        vUv = uv;
        float e = 0.004;
        float z = bump(uv) * ${AMP.toFixed(1)};
        float zx = (bump(uv + vec2(e, 0.0)) - bump(uv - vec2(e, 0.0))) * ${AMP.toFixed(1)} / (2.0 * e * ${W.toFixed(1)});
        float zy = (bump(uv + vec2(0.0, e)) - bump(uv - vec2(0.0, e))) * ${AMP.toFixed(1)} / (2.0 * e * ${H.toFixed(1)});
        vec3 n = normalize(vec3(-zx, -zy, 1.0));
        vShade = 0.9 + 0.1 * max(dot(n, normalize(vec3(0.25, 0.4, 0.88))), 0.0);
        gl_Position = projectionMatrix * modelViewMatrix * vec4(position.xy, z, 1.0);
      }
    `,
    fragmentShader: /* glsl */ `
      uniform sampler2D uMap;
      varying vec2 vUv; varying float vShade;
      void main() {
        gl_FragColor = vec4(texture2D(uMap, vUv).rgb * vShade, 1.0);
      }
    `,
  });
  const mesh = new THREE.Mesh(geo, material);
  mesh.position.set(0, 40, 0);
  scene.add(mesh);

  const mkAspectPlane = (canvas: HTMLCanvasElement, width: number) => {
    const h = (width * canvas.height) / canvas.width;
    const m = new THREE.Mesh(
      new THREE.PlaneGeometry(width, h),
      new THREE.MeshBasicMaterial({ map: canvasTexture(canvas), transparent: true }),
    );
    m.position.set(0, -60, -20);
    scene.add(m);
    return m;
  };
  const messagePlane = mkAspectPlane(
    cropCanvas(imgs.message, rects.message.userMessage),
    900,
  );
  const planPlane = mkAspectPlane(
    cropCanvas(imgs.plan, rects.plan.planTight ?? rects.plan.plan),
    980,
  );

  return {
    scene,
    camera,
    render(local) {
      // 一次压力：0.8s 起波，2.0s 后指数消散，3.6s 前完全平息（幅值 80px，可见连续形变）。
      const t0 = 800;
      const age = local - t0;
      const env = age < 0 ? 0 : Math.exp(-Math.max(age - 1200, 0) / 700);
      material.uniforms.uWave!.value = env;
      material.uniforms.uPhase!.value = Math.max(age, 0) * 0.014;
      // 网格平坦（|wave|<0.02）后才交接同一句子的真实消息 → 计划。
      mesh.visible = local < 4300;
      messagePlane.material.opacity = opacityWindow(local, 4300, 6100, 250);
      planPlane.material.opacity = opacityWindow(local, 6200, 8000, 250);
    },
    dispose() {
      geo.dispose();
      material.dispose();
      texC.dispose();
      scene.traverse((o) => {
        const mesh = o as THREE.Mesh;
        mesh.geometry?.dispose();
        (mesh.material as THREE.Material | undefined)?.dispose();
      });
    },
  };
}

/* ------------------------------------------------------------------ */
/* 镜头 04：原生计划的空间折页 —— 真实步骤文字沿左 hinge 连续弯曲。 */
function buildScene04(imgs: Record<NativeState, HTMLImageElement>, rects: StyleInputs["rects"]): SceneKit {
  const scene = new THREE.Scene();
  scene.background = new THREE.Color("#101114");
  const camera = new THREE.PerspectiveCamera(34, 1920 / 1080, 10, 8000);
  camera.position.set(0, 0, 1500);
  camera.lookAt(0, 0, 0);

  const tightRows = (rects.plan.planRowsTight ?? rects.plan.planRows ?? []).slice(0, 3);
  const rowMeshes: Array<{ material: THREE.ShaderMaterial }> = [];
  const LEFT = 420;
  // 世界 y：读取在上、确认居中、提交在下（DOM 顺序即执行顺序）。
  const CENTERS = [180, 20, -140];
  tightRows.forEach((rect, index) => {
    // tight 裁剪 → 白底转透明、黑字转白 alpha：暗底只见真实步骤字形。
    const canvas = inkToWhite(cropCanvas(imgs.plan, rect));
    const tex = canvasTexture(canvas);
    const height = 86;
    const width = (height * canvas.width) / canvas.height;
    const geo = new THREE.PlaneGeometry(width, height, 72, 12);
    geo.translate(width / 2, 0, 0); // 局部 x ∈ [0, width]，hinge 在左缘。
    const material = new THREE.ShaderMaterial({
      transparent: true,
      depthWrite: false,
      side: THREE.DoubleSide,
      uniforms: { uMap: { value: tex }, uK: { value: 0 }, uOpacity: { value: 0 } },
      vertexShader: /* glsl */ `
        uniform float uK;
        varying vec2 vUv; varying float vShade;
        void main() {
          vUv = uv;
          float s = position.x;
          float th = s * uK;
          float x = abs(uK) < 1e-7 ? s : sin(th) / uK;
          float z = abs(uK) < 1e-7 ? 0.0 : (1.0 - cos(th)) / uK;
          float e = 8.0;
          float thm = (s - e) * uK;
          float thp = (s + e) * uK;
          float xm = abs(uK) < 1e-7 ? s - e : sin(thm) / uK;
          float zm = abs(uK) < 1e-7 ? 0.0 : (1.0 - cos(thm)) / uK;
          float xp = abs(uK) < 1e-7 ? s + e : sin(thp) / uK;
          float zp = abs(uK) < 1e-7 ? 0.0 : (1.0 - cos(thp)) / uK;
          vec3 tangent = normalize(vec3(xp - xm, 0.0, zp - zm));
          vec3 n = normalize(cross(tangent, vec3(0.0, 1.0, 0.0)));
          if (n.z < 0.0) n = -n;
          vShade = 0.78 + 0.22 * max(dot(n, normalize(vec3(0.3, 0.5, 0.81))), 0.0);
          gl_Position = projectionMatrix * modelViewMatrix * vec4(x, position.y, z, 1.0);
        }
      `,
      fragmentShader: /* glsl */ `
        uniform sampler2D uMap; uniform float uOpacity;
        varying vec2 vUv; varying float vShade;
        void main() {
          vec4 t = texture2D(uMap, vUv);
          if (t.a < 0.02) discard;
          gl_FragColor = vec4(vec3(1.0) * vShade, t.a * uOpacity);
        }
      `,
    });
    const mesh = new THREE.Mesh(geo, material);
    mesh.position.set(LEFT - 960, CENTERS[index]!, 20 + index * 8);
    scene.add(mesh);
    rowMeshes.push({ material });
  });

  // 归整后的原生计划证据：整个计划 tight 范围 aspect fit，真实卡片原样。
  const planTight = cropCanvas(imgs.plan, rects.plan.planTight ?? rects.plan.plan);
  const planWidth = 940;
  const planMesh = new THREE.Mesh(
    new THREE.PlaneGeometry(planWidth, (planWidth * planTight.height) / planTight.width),
    new THREE.MeshBasicMaterial({ map: canvasTexture(planTight), transparent: true }),
  );
  planMesh.position.set(0, -20, 0);
  scene.add(planMesh);

  return {
    scene,
    camera,
    render(local) {
      // 曲率峰值 R ≥ width/1.2：单行最多弯 ~1.2rad，绝不卷几圈自遮。
      rowMeshes.forEach(({ material }, index) => {
        const stagger = index * 150;
        const out = easeInOut(seg(local, 300 + stagger, 1300 + stagger));
        const hold = 1 - easeInOut(seg(local, 2400 + stagger, 3400 + stagger));
        material.uniforms.uK!.value = (1.2 / 780) * out * hold;
        material.uniforms.uOpacity!.value = opacityWindow(local, 150, 3550, 200);
      });
      planMesh.material.opacity = opacityWindow(local, 3600, 8000, 250);
    },
    dispose() {
      scene.traverse((o) => {
        const mesh = o as THREE.Mesh;
        mesh.geometry?.dispose();
        (mesh.material as THREE.Material | undefined)?.dispose();
      });
    },
  };
}

/* ------------------------------------------------------------------ */
/* 镜头 05：局部光学 —— 高度场梯度折射 + 微色散 + Fresnel，区域 aspect 保持。 */
function buildScene05(imgs: Record<NativeState, HTMLImageElement>, rects: StyleInputs["rects"]): SceneKit {
  const scene = new THREE.Scene();
  scene.background = new THREE.Color(BG_BLACK);
  const camera = orthoCamera();
  camera.position.z = 100;

  const approvalRect = rects.approval.approval ?? { x: 0.3, y: 0.55, w: 0.4, h: 0.2 };
  const allow = rects.approval.approvalAllow ?? { x: 0.4, y: 0.7, w: 0.1, h: 0.06 };
  const receipt = rects.submitted.receipt ?? { x: 0.3, y: 0.5, w: 0.4, h: 0.2 };
  // A = 审批近景，B = 已提交回执；各自 aspect 的居中显示区（宽 ≤1400）。
  const regionFor = (rect: StyleRect) => {
    const aspect = (rect.w * NATIVE_W) / (rect.h * NATIVE_H);
    const width = Math.min(1400, 1400);
    const height = Math.round(width / aspect);
    const x = (1920 - width) / 2;
    const y = 190 + (820 - 190 - height) / 2 + 30;
    return { x, y, width, height, aspect };
  };
  const regionA = regionFor(approvalRect);
  const regionB = regionFor(receipt);
  const cropA = cropCanvas(imgs.approval, approvalRect);
  const cropB = cropCanvas(imgs.submitted, receipt);
  const texA = canvasTexture(cropA);
  const texB = canvasTexture(cropB);
  // press：真实 allow rect 中心 → 审批裁剪 uv → regionA uv。
  const pressU = (allow.x + allow.w / 2 - approvalRect.x) / approvalRect.w;
  const pressV = 1 - (allow.y + allow.h / 2 - approvalRect.y) / approvalRect.h;
  const press = new THREE.Vector2(pressU, pressV);
  const sdf = signedDistanceField("允许", 240);
  const sdfTex = sdfTexture(sdf);
  const material = new THREE.ShaderMaterial({
    uniforms: {
      uA: { value: texA },
      uB: { value: texB },
      uMix: { value: 0 },
      uSdf: { value: sdfTex },
      uPress: { value: press },
      uStrength: { value: 0 },
      uRadius: { value: 0.2 },
      uRegionA: {
        value: new THREE.Vector4(
          regionA.x / 1920,
          1 - (regionA.y + regionA.height) / 1080,
          regionA.width / 1920,
          regionA.height / 1080,
        ),
      },
      uRegionB: {
        value: new THREE.Vector4(
          regionB.x / 1920,
          1 - (regionB.y + regionB.height) / 1080,
          regionB.width / 1920,
          regionB.height / 1080,
        ),
      },
    },
    vertexShader: /* glsl */ `
      varying vec2 vUv;
      void main() { vUv = uv; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0); }
    `,
    fragmentShader: /* glsl */ `
      uniform sampler2D uA; uniform sampler2D uB; uniform sampler2D uSdf;
      uniform vec2 uPress; uniform float uStrength; uniform float uRadius; uniform float uMix;
      uniform vec4 uRegionA; uniform vec4 uRegionB;
      varying vec2 vUv;
      // 字形高度场：允许字形 SDF 约束在按钮附近的不规则局部隆起。
      float height(vec2 regionUv) {
        vec2 g = (regionUv - (uPress - vec2(0.10, 0.06))) / vec2(0.20, 0.12);
        float m = 0.0;
        if (g.x >= 0.0 && g.x <= 1.0 && g.y >= 0.0 && g.y <= 1.0) {
          float d = texture2D(uSdf, vec2(g.x, 1.0 - g.y)).r;
          m = smoothstep(14.0, -14.0, d);
        }
        float r = length((regionUv - uPress) * vec2(1.0, 0.62));
        return uStrength * m * exp(-pow(r / uRadius, 2.0));
      }
      vec3 sampleUI(vec2 uv) {
        vec4 ra = uRegionA; vec4 rb = uRegionB;
        vec2 a = (uv - ra.xy) / ra.zw;
        vec2 b = (uv - rb.xy) / rb.zw;
        vec3 col = vec3(0.016, 0.02, 0.024);
        if (a.x >= 0.0 && a.x <= 1.0 && a.y >= 0.0 && a.y <= 1.0)
          col = texture2D(uA, a).rgb;
        if (b.x >= 0.0 && b.x <= 1.0 && b.y >= 0.0 && b.y <= 1.0)
          col = mix(col, texture2D(uB, b).rgb, uMix);
        return col;
      }
      void main() {
        vec4 ra = uRegionA;
        vec2 a = (vUv - ra.xy) / ra.zw;
        float ex = 1.2 / ra.z;
        float ey = 1.2 / ra.w;
        // x/y 各自方向的数值梯度。
        float hx = height(a + vec2(ex, 0.0)) - height(a - vec2(ex, 0.0));
        float hy = height(a + vec2(0.0, ey)) - height(a - vec2(0.0, ey));
        // 折射位移（屏幕空间），各轴上限 16px。
        vec2 off = clamp(vec2(hx, hy) * 0.9, vec2(-16.0 / 1920.0, -16.0 / 1080.0), vec2(16.0 / 1920.0, 16.0 / 1080.0));
        vec3 col;
        col.r = sampleUI(vUv + off * 0.985).r;
        col.g = sampleUI(vUv + off).g;
        col.b = sampleUI(vUv + off * 1.015).b;
        col += vec3(0.9, 0.95, 1.0) * pow(clamp(height(a) * 1.5, 0.0, 1.0), 2.0) * 0.3;
        gl_FragColor = vec4(col, 1.0);
      }
    `,
  });
  scene.add(new THREE.Mesh(new THREE.PlaneGeometry(1920, 1080), material));

  return {
    scene,
    camera,
    render(local) {
      // 2.0s 按下：0.8s 折射收束（半径收缩、强度先升后归零），随后回原像素。
      const t0 = 2000;
      const age = local - t0;
      const env = age < 0 || age > 800 ? 0 : Math.sin(Math.PI * (age / 800));
      material.uniforms.uStrength!.value = env;
      material.uniforms.uRadius!.value = 0.22 - 0.1 * easeOut(clamp01(age / 800));
      // 5.0–5.3 在零高度场时切到已提交回执（可读，不空白）。
      material.uniforms.uMix!.value = easeInOut(seg(local, 5000, 5300));
    },
    dispose() {
      material.dispose();
      [texA, texB, sdfTex].forEach((t) => t.dispose());
    },
  };
}

/* ------------------------------------------------------------------ */

const BUILDERS = [
  buildScene01,
  buildScene02,
  buildScene03,
  buildScene04,
  buildScene05,
] as const;

const SCENE_TITLES = [
  "01 黑白文字曲面",
  "02 白场字形轮廓",
  "03 冷蓝界面弹性",
  "04 原生计划折页",
  "05 局部光学折射",
];

export function StyleLab() {
  const t = useClock();
  const preset = Math.min(STYLE_SCENES - 1, Math.floor(t / STYLE_SCENE_MS));
  const local = t - preset * STYLE_SCENE_MS;
  const [inputs] = useState<StyleInputs>(() => {
    const value = window.__STYLE_INPUTS__;
    if (!value)
      throw new Error("缺少 window.__STYLE_INPUTS__：请用 design-lab.mjs 或 design-preview.html 打开。");
    return value;
  });
  const [images, setImages] = useState<Record<NativeState, HTMLImageElement> | null>(null);
  const [error, setError] = useState<string>();
  const hostRef = useRef<HTMLDivElement>(null);
  const rendererRef = useRef<THREE.WebGLRenderer | null>(null);
  const kitRef = useRef<SceneKit | null>(null);
  const readyRef = useRef(false);

  useEffect(() => {
    let cancelled = false;
    Promise.all(
      LOAD_STATES.map(async (state) => [state, await loadImage(inputs.images[state])] as const),
    )
      .then((entries) => {
        if (cancelled) return;
        setImages(Object.fromEntries(entries) as Record<NativeState, HTMLImageElement>);
      })
      .catch((e: unknown) => setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [inputs]);

  useEffect(() => {
    if (error || !images || !hostRef.current) return;
    try {
      const renderer = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true });
      renderer.setSize(1920, 1080);
      renderer.setPixelRatio(1);
      hostRef.current.appendChild(renderer.domElement);
      rendererRef.current = renderer;
    } catch (e) {
      setError(`WebGL 初始化失败：${String(e)}`);
      return;
    }
    return () => {
      rendererRef.current?.dispose();
      rendererRef.current = null;
    };
  }, [error, images]);

  useEffect(() => {
    const renderer = rendererRef.current;
    if (error || !renderer || !images) return;
    kitRef.current?.dispose();
    kitRef.current = BUILDERS[preset]!(images, inputs.rects);
  }, [preset, error, images, inputs]);

  useEffect(() => {
    const renderer = rendererRef.current;
    const kit = kitRef.current;
    if (error || !renderer || !kit) return;
    kit.render(local);
    renderer.render(kit.scene, kit.camera);
    if (!readyRef.current && local >= 0) {
      readyRef.current = true;
      document.fonts.ready.then(() => {
        window.__STYLE_READY__ = true;
      });
    }
  }, [local, preset, error, images]);

  // 原生证据（02/03）：直接用 cropCanvas 产物做 dataURL，避免 background 尺寸换算。
  const inset = useMemo(() => {
    if (!images) return null;
    const rect = inputs.rects.plan.planTight ?? inputs.rects.plan.plan;
    const canvas = cropCanvas(images.plan, rect);
    const width = 900;
    return {
      url: canvas.toDataURL("image/png"),
      width,
      height: (width * canvas.height) / canvas.width,
    };
  }, [images, inputs]);

  if (error) {
    return (
      <div className="style-lab-error" role="alert">
        <h2>风格试验台无法渲染</h2>
        <p>{error}</p>
        <p>本页不做降级假渲染：WebGL 或输入纹理缺失时直接报错。</p>
      </div>
    );
  }

  const planInsetOpacity =
    preset === 1 ? opacityWindow(local, 5000, 8000, 250) : preset === 2 ? 0 : 0;

  return (
    <div className="style-lab" data-style={String(preset + 1).padStart(2, "0")}>
      <div ref={hostRef} className="style-lab-canvas-host" />
      {preset === 1 && planInsetOpacity > 0 && inset && (
        <img
          className="style-lab-inset"
          src={inset.url}
          style={{ opacity: planInsetOpacity, width: inset.width, height: inset.height }}
          alt=""
        />
      )}
      {preset === 1 && (
        <div className="style-lab-steps">
          {["目标", "确认", "执行"].map((label, index) => {
            const active =
              local < 2600 ? index === 0 : local < 4100 ? index <= 1 : index <= 2;
            const current =
              local < 2600 ? index === 0 : local < 5000 ? index === 1 : index === 2;
            return (
              <span
                key={label}
                className="style-lab-step"
                data-active={active ? "1" : undefined}
                data-current={current ? "1" : undefined}
              >
                {label}
              </span>
            );
          })}
        </div>
      )}
      {preset === 2 && (
        <div className="style-lab-heading">输入目标。</div>
      )}
      {preset === 4 && (
        <div className="style-lab-heading">操作之前，先确认。</div>
      )}
      <div className="style-lab-caption">{SCENE_TITLES[preset]}</div>
    </div>
  );
}
