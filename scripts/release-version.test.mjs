import test from 'node:test';
import assert from 'node:assert/strict';
import { parseVersion, compareVersions, validateChannel } from '../cloudflare/version.mjs';
import { releaseConfig } from './release-version.mjs';

test('版本大小与正式测试通道互相独立', () => {
  assert.equal(validateChannel('0.0.1', 'stable'), 'stable');
  assert.equal(validateChannel('1.0.0', 'test'), 'test');
  assert.throws(() => validateChannel('0.1.0', undefined));
  assert.throws(() => validateChannel('0.1.0', 'unknown'));
  for (const stage of ['alpha', 'beta', 'rc']) {
    assert.equal(validateChannel(`0.2.0-${stage}.1`, 'test'), 'test');
    assert.throws(() => validateChannel(`0.2.0-${stage}.1`, 'stable'));
  }
});

test('SemVer 排序支持数字测试序号和测试转正式', () => {
  const versions = ['0.1.9', '0.1.10', '0.2.0-alpha.2', '0.2.0-alpha.10', '0.2.0-beta.1', '0.2.0-rc.1', '0.2.0'];
  for (let i = 1; i < versions.length; i++) {
    assert.equal(compareVersions(versions[i - 1], versions[i]), -1);
    assert.equal(compareVersions(versions[i], versions[i - 1]), 1);
  }
  assert.equal(compareVersions('0.2.0+one', '0.2.0+two'), 0);
});

test('非法版本和路径不能通过发布校验', () => {
  for (const version of ['v0.1.0', '0.01.0', '0.1.0-alpha.01', '0.1.0/evil', '0.1.0-', '0.1.0+', '18446744073709551616.0.0'])
    assert.throws(() => parseVersion(version), version);
});

test('当前发布配置默认正式通道，手动覆盖保持显式', async () => {
  const configured = await releaseConfig();
  assert.equal(configured.prerelease, configured.channel === 'test');
  assert.equal((await releaseConfig('test', true)).prerelease, true);
  if (parseVersion(configured.version).pre.length) await assert.rejects(releaseConfig('stable', true));
  else assert.equal((await releaseConfig('stable', true)).prerelease, false);
});

test('上传不允许只改环境变量而让构建属性与发布属性不一致', async () => {
  const config = await releaseConfig();
  await assert.rejects(releaseConfig(config.channel === 'stable' ? 'test' : 'stable'));
});
