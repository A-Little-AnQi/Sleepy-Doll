// 确定性 3D 投影：透视相机、逐面朗伯+高光、平行光地面解析投影。
// 全部函数只依赖输入值，不用 Date.now，供逐帧 seek 复算。
export interface Vec3 {
  x: number;
  y: number;
  z: number;
}

export const v3 = (x: number, y: number, z: number): Vec3 => ({ x, y, z });
export const add = (a: Vec3, b: Vec3): Vec3 => v3(a.x + b.x, a.y + b.y, a.z + b.z);
export const sub = (a: Vec3, b: Vec3): Vec3 => v3(a.x - b.x, a.y - b.y, a.z - b.z);
export const scale = (a: Vec3, k: number): Vec3 => v3(a.x * k, a.y * k, a.z * k);
export const dot = (a: Vec3, b: Vec3): number => a.x * b.x + a.y * b.y + a.z * b.z;
export const cross = (a: Vec3, b: Vec3): Vec3 =>
  v3(a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x);
export const len = (a: Vec3): number => Math.hypot(a.x, a.y, a.z);
export const norm = (a: Vec3): Vec3 => {
  const l = len(a);
  return l < 1e-9 ? v3(0, 1, 0) : scale(a, 1 / l);
};
export const lerp3 = (a: Vec3, b: Vec3, k: number): Vec3 =>
  v3(a.x + (b.x - a.x) * k, a.y + (b.y - a.y) * k, a.z + (b.z - a.z) * k);

export interface Tri {
  a: number;
  b: number;
  c: number;
}

export interface Mesh {
  verts: Vec3[];
  tris: Tri[];
}

export interface Material {
  /** sRGB 基色 0..255。 */
  color: readonly [number, number, number];
  ambient: number;
  diffuse: number;
  spec: number;
  shine: number;
}

/** 平行光：dir 指向光源。 */
export interface Light {
  dir: Vec3;
}

export interface Camera {
  eye: Vec3;
  target: Vec3;
  /** 焦距（像素）。eye 到目标平面距离等于 focal 时投影为 1:1。 */
  focal: number;
  width: number;
  height: number;
}

export interface Projected {
  x: number;
  y: number;
  z: number;
}

export function cameraBasis(camera: Camera) {
  const forward = norm(sub(camera.target, camera.eye));
  const reference = Math.abs(forward.y) > 0.98 ? v3(0, 0, 1) : v3(0, 1, 0);
  const right = norm(cross(forward, reference));
  const up = cross(right, forward);
  return { forward, right, up };
}

export function project(camera: Camera, point: Vec3): Projected | undefined {
  const { forward, right, up } = cameraBasis(camera);
  const d = sub(point, camera.eye);
  const z = dot(d, forward);
  if (z < 60) return undefined;
  const k = camera.focal / z;
  return {
    x: camera.width / 2 + dot(d, right) * k,
    y: camera.height / 2 - dot(d, up) * k,
    z,
  };
}

export function transformMesh(mesh: Mesh, fn: (p: Vec3) => Vec3): Mesh {
  return { verts: mesh.verts.map(fn), tris: mesh.tris };
}

export function rotateY(mesh: Mesh, radians: number, pivot: Vec3): Mesh {
  const cos = Math.cos(radians),
    sin = Math.sin(radians);
  return transformMesh(mesh, (p) => {
    const x = p.x - pivot.x,
      z = p.z - pivot.z;
    return v3(pivot.x + x * cos + z * sin, p.y, pivot.z - x * sin + z * cos);
  });
}

function faceColor(
  a: Vec3,
  b: Vec3,
  c: Vec3,
  material: Material,
  light: Light,
  eye: Vec3,
): string {
  const n = norm(cross(sub(b, a), sub(c, a)));
  const centroid = scale(add(add(a, b), c), 1 / 3);
  const view = norm(sub(eye, centroid));
  const facing = dot(n, view);
  if (facing < 0) {
    // 双面材质：背面翻法线，非凸实体（月牙）不会整面变黑。
    n.x = -n.x;
    n.y = -n.y;
    n.z = -n.z;
  }
  const diffuse = Math.max(0, dot(n, light.dir));
  const halfway = norm(add(light.dir, view));
  const specular = Math.pow(Math.max(0, dot(n, halfway)), material.shine);
  const channel = (base: number) =>
    Math.min(
      255,
      Math.round(
        base * (material.ambient + material.diffuse * diffuse) +
          255 * material.spec * specular,
      ),
    );
  const [red, green, blue] = material.color;
  return `rgb(${channel(red)},${channel(green)},${channel(blue)})`;
}

/** 画一个网格：透视、逐面着色、按深度从远到近。 */
export function drawMesh(
  ctx: CanvasRenderingContext2D,
  mesh: Mesh,
  material: Material,
  camera: Camera,
  light: Light,
  alpha = 1,
) {
  const projected = mesh.verts.map((p) => project(camera, p));
  const order = mesh.tris
    .map((tri, index) => {
      const a = projected[tri.a],
        b = projected[tri.b],
        c = projected[tri.c];
      if (!a || !b || !c) return undefined;
      return { index, depth: a.z + b.z + c.z };
    })
    .filter((item): item is { index: number; depth: number } => !!item)
    .sort((p, q) => q.depth - p.depth);
  ctx.globalAlpha = alpha;
  for (const { index } of order) {
    const tri = mesh.tris[index]!;
    const a = mesh.verts[tri.a]!,
      b = mesh.verts[tri.b]!,
      c = mesh.verts[tri.c]!;
    ctx.fillStyle = faceColor(a, b, c, material, light, camera.eye);
    const pa = projected[tri.a]!,
      pb = projected[tri.b]!,
      pc = projected[tri.c]!;
    ctx.beginPath();
    ctx.moveTo(pa.x, pa.y);
    ctx.lineTo(pb.x, pb.y);
    ctx.lineTo(pc.x, pc.y);
    ctx.closePath();
    ctx.fill();
  }
  ctx.globalAlpha = 1;
}

/** 平行光把网格投影到 y=floorY 的地面，画成一张柔化阴影。 */
export function drawShadow(
  ctx: CanvasRenderingContext2D,
  mesh: Mesh,
  camera: Camera,
  light: Light,
  floorY: number,
  alpha: number,
  blurPx: number,
) {
  // 光源不在地平线上方（dir.y 指向光源）时没有地面投影，跳过。
  if (light.dir.y < 0.05) return;
  const flat = transformMesh(mesh, (p) => {
    const t = (floorY - p.y) / light.dir.y;
    return v3(p.x + light.dir.x * t, floorY, p.z + light.dir.z * t);
  });
  ctx.save();
  ctx.globalAlpha = alpha;
  ctx.filter = `blur(${blurPx.toFixed(1)}px)`;
  ctx.fillStyle = "#04060c";
  for (const tri of flat.tris) {
    const a = project(camera, flat.verts[tri.a]!),
      b = project(camera, flat.verts[tri.b]!),
      c = project(camera, flat.verts[tri.c]!);
    if (!a || !b || !c) continue;
    ctx.beginPath();
    ctx.moveTo(a.x, a.y);
    ctx.lineTo(b.x, b.y);
    ctx.lineTo(c.x, c.y);
    ctx.closePath();
    ctx.fill();
  }
  ctx.restore();
}

/** 四点双线性细分贴图：浅视角下的投影近似，主视角接近正面时误差不可见。 */
export function drawTexturedQuad(
  ctx: CanvasRenderingContext2D,
  image: CanvasImageSource,
  imageWidth: number,
  imageHeight: number,
  corners: [Projected, Projected, Projected, Projected],
  columns = 10,
  alpha = 1,
) {
  ctx.save();
  ctx.globalAlpha = alpha;
  for (let i = 0; i < columns; i += 1) {
    const k0 = i / columns,
      k1 = (i + 1) / columns;
    const lerp2 = (p: Projected, q: Projected, k: number) => ({
      x: p.x + (q.x - p.x) * k,
      y: p.y + (q.y - p.y) * k,
    });
    const [tl, tr, br, bl] = corners;
    const a = lerp2(tl, tr, k0),
      b = lerp2(tl, tr, k1);
    const d = lerp2(bl, br, k0),
      c = lerp2(bl, br, k1);
    const sx = (k0 * imageWidth) | 0;
    const sw = Math.max(1, ((k1 - k0) * imageWidth) | 0);
    ctx.save();
    ctx.beginPath();
    ctx.moveTo(a.x, a.y);
    ctx.lineTo(b.x, b.y);
    ctx.lineTo(c.x, c.y);
    ctx.lineTo(d.x, d.y);
    ctx.closePath();
    ctx.clip();
    // 用左上/左下边的仿射基把贴图条映射进四边形。
    const ux = (b.x - a.x) / imageWidth,
      uy = (b.y - a.y) / imageWidth;
    const vx = (d.x - a.x) / imageHeight,
      vy = (d.y - a.y) / imageHeight;
    ctx.transform(ux, uy, vx, vy, a.x, a.y);
    ctx.drawImage(image, sx, 0, sw, imageHeight, 0, 0, imageWidth, imageHeight);
    ctx.restore();
  }
  ctx.restore();
}
