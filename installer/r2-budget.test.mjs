import test from "node:test";
import assert from "node:assert/strict";
import { parseListing, checkStorageBudget, MAX_BUCKET_BYTES } from "./r2-budget.mjs";
test("容量检查包含已有文件与本次新增，并正确处理覆盖", () => {
  const objects = [{ key: "old", size: 7, storageClass: "STANDARD" }];
  assert.deepEqual(checkStorageBudget(objects, [{ key: "new", size: 9 }, { key: "old", size: 8 }]), { before: 7, after: 17, limit: MAX_BUCKET_BYTES });
  assert.throws(() => checkStorageBudget(objects, [{ key: "new", size: MAX_BUCKET_BYTES }]), /停止发布/);
  assert.throws(() => checkStorageBudget([{ key: "old", size: MAX_BUCKET_BYTES + 1, storageClass: "STANDARD" }], [{ key: "old", size: 1 }]), /停止发布/);
});
test("非免费存储类型和重复分页对象被拒绝", () => {
  assert.throws(() => checkStorageBudget([{ key: "x", size: 1, storageClass: "STANDARD_IA" }]), /非 Standard/);
  assert.throws(() => checkStorageBudget([{ key: "x", size: 1, storageClass: "STANDARD" }, { key: "x", size: 2, storageClass: "STANDARD" }]), /重复/);
});
test("分页清单解析计数和 XML 转义，缺失信息时不继续上传", () => {
  const value = parseListing('<ListBucketResult><KeyCount>1</KeyCount><IsTruncated>true</IsTruncated><NextContinuationToken>a&amp;b</NextContinuationToken><Contents><Key>x&amp;y</Key><Size>12</Size><StorageClass>STANDARD</StorageClass></Contents></ListBucketResult>');
  assert.equal(value.objects[0].key, "x&y"); assert.equal(value.next, "a&b");
  assert.throws(() => parseListing('<ListBucketResult><IsTruncated>true</IsTruncated></ListBucketResult>'), /分页信息/);
  assert.throws(() => parseListing('<ListBucketResult><IsTruncated>false</IsTruncated><Contents><Key>x</Key></Contents></ListBucketResult>'), /容量无效/);
});
