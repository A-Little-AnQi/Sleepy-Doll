// 品牌月牙与金星的立体几何。比例取自 web/src/brand/icon.svg（viewBox 22，y 向下）：
// 月牙 = 外圆 c(11,11) r9 挖内圆 c(15.19,11) r6.5，两圆交点 (17.72,5.01)/(17.72,16.99)，
// 整组绕 (11,11) 旋转 -35°；金星为四角星，中心 ≈(17.2,11)，
// 端点 E(24,11)/N(16.6,5.9)/W(11.5,11)/S(16.6,16.1)，凹边按 Q 曲线中点收拢。
import type { Mesh, Tri, Vec3 } from "./spatial";
import { v3 } from "./spatial";

const CENTER = { x: 11, y: 11 };
const OUTER_R = 9;
const INNER_R = 6.5;
const INNER_C = { x: 15.19, y: 11 };
// 交点相对外圆的夹角（y 向上坐标系里为 ∓41.7°）。
const TIP_ANGLE = (41.7 * Math.PI) / 180;
const GROUP_ROT = (-35 * Math.PI) / 180;

/** icon.svg 的 y 向下坐标 → 绕月牙中心旋转 -35° → 舞台 y 向上局部坐标。 */
function svgPoint(x: number, y: number, unit: number): Vec3 {
  const dx = x - CENTER.x,
    dy = y - CENTER.y;
  const cos = Math.cos(GROUP_ROT),
    sin = Math.sin(GROUP_ROT);
  const rx = dx * cos - dy * sin;
  const ry = dx * sin + dy * cos;
  return v3(rx * unit, -ry * unit, 0);
}

function prism(ring: Vec3[], depth: number): Mesh {
  const verts: Vec3[] = [];
  const tris: Tri[] = [];
  for (const p of ring) verts.push({ ...p, z: depth / 2 });
  for (const p of ring) verts.push({ ...p, z: -depth / 2 });
  const n = ring.length;
  for (let i = 0; i < n; i += 1) {
    const j = (i + 1) % n;
    tris.push({ a: i, b: j, c: n + j });
    tris.push({ a: i, b: n + j, c: n + i });
    tris.push({ a: i, b: n + j, c: j });
    tris.push({ a: i, b: n + i, c: n + j });
  }
  return { verts, tris };
}

/** size = 外圆半径（舞台像素）。 */
export function crescentMesh(size: number, depth: number): Mesh {
  const unit = size / OUTER_R;
  const samples = 40;
  const ring: Vec3[] = [];
  const innerTip = Math.acos(
    (INNER_C.x - CENTER.x) / INNER_R,
  ); // 交点相对内圆的夹角
  // 外弧：上交点逆时针经 180° 到下交点（主弧）。
  for (let i = 0; i <= samples; i += 1) {
    const a = TIP_ANGLE + ((Math.PI * 2 - 2 * TIP_ANGLE) * i) / samples;
    ring.push(
      svgPoint(
        CENTER.x + Math.cos(a) * OUTER_R,
        CENTER.y - Math.sin(a) * OUTER_R,
        unit,
      ),
    );
  }
  // 内弧：下交点经 0° 回到上交点（凹边）。
  for (let i = samples; i >= 0; i -= 1) {
    const a = -innerTip + ((2 * innerTip) * i) / samples;
    ring.push(
      svgPoint(
        INNER_C.x + Math.cos(a) * INNER_R,
        INNER_C.y - Math.sin(a) * INNER_R,
        unit,
      ),
    );
  }
  return prism(ring, depth);
}

/** size = 星端点平均半径（舞台像素）。 */
export function starMesh(size: number, depth: number): Mesh {
  const unit = size / 6.0;
  const tips: Array<[number, number]> = [
    [24, 11],
    [16.6, 5.9],
    [11.5, 11],
    [16.6, 16.1],
  ];
  const ring: Vec3[] = [];
  for (let k = 0; k < 4; k += 1) {
    const tip = tips[k]!;
    const next = tips[(k + 1) % 4]!;
    const mid = { x: (tip[0] + next[0]) / 2, y: (tip[1] + next[1]) / 2 };
    const toward = {
      x: CENTER.x + (mid.x - CENTER.x) * 0.42,
      y: CENTER.y + (mid.y - CENTER.y) * 0.42,
    };
    // Q 曲线近似：端点—中谷—下一端点三段折线。
    ring.push(svgPoint(tip[0], tip[1], unit));
    ring.push(svgPoint((tip[0] + toward.x) / 2, (tip[1] + toward.y) / 2, unit));
    ring.push(svgPoint(toward.x, toward.y, unit));
    ring.push(svgPoint((next[0] + toward.x) / 2, (next[1] + toward.y) / 2, unit));
  }
  return prism(ring, depth);
}
