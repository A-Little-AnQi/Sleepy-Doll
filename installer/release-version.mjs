import { readFile, writeFile, appendFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
import { validateChannel } from '../cloudflare/version.mjs';

export async function releaseConfig(override, allowOverride = false) {
  const version = JSON.parse(await readFile('package.json', 'utf8')).version;
  const cargo = /^version\s*=\s*"([^"]+)"/m.exec(await readFile('Cargo.toml', 'utf8'))?.[1];
  if (version !== cargo) throw Error('前端和 Cargo 版本号必须一致');
  const config = JSON.parse(await readFile('release-channel.json', 'utf8'));
  if (override && override !== config.channel && !allowOverride)
    throw Error('发布通道与构建配置不一致，请先修改 release-channel.json；Actions 手动发布会同步配置');
  const channel = validateChannel(version, override || config.channel);
  return { version, channel, tag: `v${version}`, prerelease: channel === 'test' };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const config = await releaseConfig(process.env.RELEASE_CHANNEL, process.argv.includes('--github'));
  if (process.env.GITHUB_REF_TYPE === 'tag' && process.env.GITHUB_REF_NAME !== config.tag)
    throw Error('标签与源码版本不一致');
  if (process.argv.includes('--github')) {
    if (!process.env.GITHUB_ENV) throw Error('此参数仅用于 GitHub Actions');
    // Manual selection becomes part of the build, so its default update channel agrees with publication.
    await writeFile('release-channel.json', JSON.stringify({ channel: config.channel }, null, 2) + '\n');
    await appendFile(process.env.GITHUB_ENV, `VERSION=${config.version}\nRELEASE_TAG=${config.tag}\nRELEASE_CHANNEL=${config.channel}\n`);
  }
  console.log(JSON.stringify(config));
}
