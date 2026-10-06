// SemVer grammar, shared by the download gateway and publication tools.
const core = '(?:0|[1-9]\\d*)';
const pre = '(?:0|[1-9]\\d*|\\d*[A-Za-z-][0-9A-Za-z-]*)';
export const VERSION = new RegExp(`^(${core})\\.(${core})\\.(${core})(?:-(${pre}(?:\\.${pre})*))?(?:\\+([0-9A-Za-z-]+(?:\\.[0-9A-Za-z-]+)*))?$`);

export function parseVersion(value) {
  const match = typeof value === 'string' && value.length <= 128 && VERSION.exec(value);
  if (!match || match.slice(1, 4).some(part => BigInt(part) > 18446744073709551615n))
    throw Error('版本号必须符合 SemVer，例如 0.1.0 或 0.2.0-alpha.1');
  return { core: match.slice(1, 4).map(BigInt), pre: match[4]?.split('.') ?? [] };
}

export function compareVersions(left, right) {
  const a = parseVersion(left), b = parseVersion(right);
  const compare = (x, y) => x === y ? 0 : x > y ? 1 : -1;
  for (let i = 0; i < 3; i++) {
    const result = compare(a.core[i], b.core[i]);
    if (result) return result;
  }
  if (!a.pre.length || !b.pre.length) return compare(!a.pre.length, !b.pre.length);
  for (let i = 0; i < Math.max(a.pre.length, b.pre.length); i++) {
    if (a.pre[i] === undefined || b.pre[i] === undefined)
      return compare(a.pre[i] !== undefined, b.pre[i] !== undefined);
    const x = a.pre[i], y = b.pre[i];
    const nx = /^\d+$/.test(x), ny = /^\d+$/.test(y);
    const result = nx && ny ? compare(BigInt(x), BigInt(y)) : nx !== ny ? compare(!nx, !ny) : compare(x, y);
    if (result) return result;
  }
  return 0;
}

export function validateChannel(version, channel) {
  const parsed = parseVersion(version);
  if (!['stable', 'test'].includes(channel)) throw Error('必须明确指定 stable 或 test 发布通道');
  if (channel === 'stable' && parsed.pre.length) throw Error('Alpha、Beta、RC 等预发布版本不能进入正式通道');
  return channel;
}
