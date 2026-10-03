// 金带与纸带：沿 Catmull-Rom 曲线扫掠的条带网格；头尾推进用参数区间裁剪。
import type { Camera, Mesh, Tri, Vec3 } from "./spatial";
import { cross, norm, project, sub, v3 } from "./spatial";

/** Catmull-Rom 采样，端点复制一份做边界。 */
export function sampleCurve(points: Vec3[], count: number): Vec3[] {
  const p = [points[0]!, ...points, points[points.length - 1]!];
  const out: Vec3[] = [];
  for (let i = 0; i < count; i += 1) {
    const t = (i / (count - 1)) * (points.length - 1);
    const seg = Math.min(points.length - 2, Math.floor(t));
    const k = t - seg;
    const a = p[seg]!,
      b = p[seg + 1]!,
      c = p[seg + 2]!,
      d = p[seg + 3]!;
    const k2 = k * k,
      k3 = k2 * k;
    out.push(
      v3(
        0.5 *
          (2 * b.x + (c.x - a.x) * k + (2 * a.x - 5 * b.x + 4 * c.x - d.x) * k2 + (3 * b.x - a.x - 3 * c.x + d.x) * k3),
        0.5 *
          (2 * b.y + (c.y - a.y) * k + (2 * a.y - 5 * b.y + 4 * c.y - d.y) * k2 + (3 * b.y - a.y - 3 * c.y + d.y) * k3),
        0.5 *
          (2 * b.z + (c.z - a.z) * k + (2 * a.z - 5 * b.z + 4 * c.z - d.z) * k2 + (3 * b.z - a.z - 3 * c.z + d.z) * k3),
      ),
    );
  }
  return out;
}

/** 金带：宽度沿 u 渐变（halfWidth(u)），沿法线压出厚度。 */
export function bandMesh(samples: Vec3[], halfWidth: (u: number) => number, thickness: number): Mesh {
  const verts: Vec3[] = [];
  const tris: Tri[] = [];
  const n = samples.length;
  for (let i = 0; i < n; i += 1) {
    const prev = samples[Math.max(0, i - 1)]!;
    const next = samples[Math.min(n - 1, i + 1)]!;
    const tangent = norm(sub(next, prev));
    const right = norm(cross(tangent, v3(0, 1, 0)));
    const normal = cross(right, tangent);
    const u = i / (n - 1);
    const w = halfWidth(u);
    const p = samples[i]!;
    verts.push(
      v3(p.x + right.x * w - normal.x * thickness, p.y + right.y * w - normal.y * thickness, p.z + right.z * w - normal.z * thickness),
      v3(p.x - right.x * w - normal.x * thickness, p.y - right.y * w - normal.y * thickness, p.z - right.z * w - normal.z * thickness),
      v3(p.x - right.x * w, p.y - right.y * w, p.z - right.z * w),
      v3(p.x + right.x * w, p.y + right.y * w, p.z + right.z * w),
    );
  }
  for (let i = 0; i < n - 1; i += 1) {
    const a = i * 4,
      b = (i + 1) * 4;
    // 顶面与两侧面；底面省略（视角始终从上方看）。
    tris.push({ a: a + 3, b: b + 3, c: b + 2 });
    tris.push({ a: a + 3, b: b + 2, c: a + 2 });
    tris.push({ a: a, b: a + 2, c: b + 2 });
    tris.push({ a: a, b: b + 2, c: b });
    tris.push({ a: a + 1, b: b + 1, c: b + 2 });
    tris.push({ a: a + 1, b: b + 2, c: a + 2 });
  }
  return { verts, tris };
}

/** 纸带贴图：整条句子画进一张横向画布，逐段仿射贴回投影四边形。
 *  halfWidth 是屏幕像素半宽；相机贴近正面时透视误差不可见。 */
export function drawTexturedStrip(
  ctx: CanvasRenderingContext2D,
  samples: Vec3[],
  halfWidthScreen: number,
  texture: HTMLCanvasElement,
  camera: Camera,
  alpha: number,
) {
  const n = samples.length;
  const projected = samples.map((p) => project(camera, p));
  const sw = texture.width / (n - 1);
  const height = texture.height;
  for (let i = 0; i < n - 1; i += 1) {
    const a = projected[i],
      b = projected[i + 1];
    if (!a || !b) continue;
    const dx = b.x - a.x,
      dy = b.y - a.y;
    const length = Math.hypot(dx, dy);
    if (length < 0.3) continue;
    const px = (-dy / length) * halfWidthScreen;
    const py = (dx / length) * halfWidthScreen;
    ctx.save();
    ctx.globalAlpha = alpha;
    ctx.beginPath();
    ctx.moveTo(a.x + px, a.y + py);
    ctx.lineTo(b.x + px, b.y + py);
    ctx.lineTo(b.x - px, b.y - py);
    ctx.lineTo(a.x - px, a.y - py);
    ctx.closePath();
    ctx.clip();
    ctx.transform(dx / sw, dy / sw, (2 * px) / height, (2 * py) / height, a.x + px, a.y + py);
    ctx.drawImage(texture, i * sw, 0, sw, height, 0, 0, sw, height);
    ctx.restore();
  }
}
