export const MAX_BUCKET_BYTES = 8_000_000_000;
const decodeXml = text => text.replace(/&#(x[0-9a-f]+|\d+);|&(amp|lt|gt|quot|apos);/gi, (_all, numeric, name) => numeric
  ? String.fromCodePoint(numeric[0].toLowerCase() === "x" ? parseInt(numeric.slice(1), 16) : Number(numeric))
  : ({ amp: "&", lt: "<", gt: ">", quot: '"', apos: "'" })[name.toLowerCase()]);
const field = (text, name) => new RegExp(`<${name}>([\\s\\S]*?)</${name}>`).exec(text)?.[1];

export function parseListing(xml) {
  if (!xml.includes("<ListBucketResult")) throw Error("R2 容量响应无效，停止发布");
  const objects = [...xml.matchAll(/<Contents>([\s\S]*?)<\/Contents>/g)].map(match => {
    const key = field(match[1], "Key"), rawSize = field(match[1], "Size");
    if (key == null || !/^\d+$/.test(rawSize ?? "")) throw Error("R2 对象容量无效，停止发布");
    const size = Number(rawSize);
    if (!Number.isSafeInteger(size)) throw Error("R2 对象容量超出有效范围");
    return { key: decodeXml(key), size, storageClass: field(match[1], "StorageClass") ?? "STANDARD" };
  });
  const keyCount = field(xml, "KeyCount");
  if (keyCount != null && Number(keyCount) !== objects.length) throw Error("R2 对象清单不完整，停止发布");
  const truncated = field(xml, "IsTruncated");
  if (!["true", "false"].includes(truncated)) throw Error("R2 分页状态无效，停止发布");
  const token = field(xml, "NextContinuationToken");
  if (truncated === "true" && !token) throw Error("R2 分页信息缺失，停止发布");
  return { objects, next: truncated === "true" ? decodeXml(token) : null };
}

export function checkStorageBudget(objects, changes = []) {
  if (objects.some(item => item.storageClass !== "STANDARD")) throw Error("发布桶存在非 Standard 对象，停止发布");
  const sizes = new Map();
  for (const item of objects) {
    if (sizes.has(item.key)) throw Error("R2 分页出现重复对象，停止发布");
    sizes.set(item.key, item.size);
  }
  const before = [...sizes.values()].reduce((sum, value) => sum + value, 0);
  for (const item of changes) {
    if (!Number.isSafeInteger(item.size) || item.size < 0) throw Error("待发布容量无效");
    sizes.set(item.key, item.size);
  }
  const after = [...sizes.values()].reduce((sum, value) => sum + value, 0);
  if (!Number.isSafeInteger(before) || !Number.isSafeInteger(after) || Math.max(before, after) > MAX_BUCKET_BYTES)
    throw Error("R2 发布桶超过 8 GB 安全上限，停止发布；已有文件与版本通道不变");
  return { before, after, limit: MAX_BUCKET_BYTES };
}
