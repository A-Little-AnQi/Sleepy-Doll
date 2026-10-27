import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { releaseConfig } from './release-version.mjs';

const { version, channel, tag } = await releaseConfig(process.env.RELEASE_CHANNEL);
const runId = process.env.UPLOADED_RUN;
const expected = process.env.UPLOADED_SHA256;
const repository = process.env.GITHUB_REPOSITORY;
const token = process.env.GH_TOKEN;
if (!/^\d+$/.test(runId ?? '') || !/^[a-f0-9]{64}$/.test(expected ?? '') || !repository || !token)
  throw Error('续接发布必须提供原运行 ID、准确 SHA-256 和 GitHub 运行环境');

async function github(path) {
  const response = await fetch(`https://api.github.com/repos/${repository}/${path}`, {
    headers: { Authorization: `Bearer ${token}`, Accept: 'application/vnd.github+json', 'X-GitHub-Api-Version': '2022-11-28' },
    signal: AbortSignal.timeout(30000),
  });
  if (!response.ok) throw Error(`无法核对原发布运行：HTTP ${response.status}`);
  return response.json();
}
const [run, jobs] = await Promise.all([github(`actions/runs/${runId}`), github(`actions/runs/${runId}/jobs?per_page=100`)]);
if (run.status !== 'completed' || run.path !== '.github/workflows/release.yml')
  throw Error('原发布运行尚未结束或不是正式发布流程');
if (!jobs.jobs.some(job => job.name === 'check / windows' && job.conclusion === 'success'))
  throw Error('原发布运行没有通过所需检查，禁止续接');
const original = jobs.jobs.find(job => job.name === 'release');
for (const name of ['Build product', 'Upload and verify R2 files', 'Deploy distribution service'])
  if (!original?.steps.some(step => step.name === name && step.conclusion === 'success'))
    throw Error(`原发布运行未完成 ${name}，禁止复用安装包`);
execFileSync('git', ['fetch', '--no-tags', 'origin', `refs/tags/${tag}:refs/tags/${tag}`], { stdio: 'pipe' });
const source = execFileSync('git', ['rev-parse', `${tag}^{}`], { encoding: 'utf8' }).trim();
if (source !== run.head_sha) throw Error('发布标签与原构建提交不一致');

const name = `Sleepy-Doll-${version}-setup.exe`;
const base = `https://sleepy-doll-download.restless-nh3.com/releases/${version}`;
const response = await fetch(`${base}/release.json`, { signal: AbortSignal.timeout(30000) });
if (!response.ok) throw Error(`原发布清单不可用：HTTP ${response.status}`);
const manifest = await response.json();
const notes = await readFile(`web/src/app/release-notes/changelog-${version}.md`, 'utf8').catch(error => {
  if (error.code === 'ENOENT') return '';
  throw error;
});
if (manifest.version !== version || manifest.channel !== channel || manifest.sha256 !== expected
  || manifest.url !== `${base}/${name}` || manifest.notes !== notes
  || !Number.isSafeInteger(manifest.size) || manifest.size <= 0 || manifest.size > 256 * 1024 * 1024)
  throw Error('原发布清单与要求续接的版本、通道、文案或摘要不一致');
const download = await fetch(manifest.url, { signal: AbortSignal.timeout(120000) });
if (!download.ok) throw Error(`原安装包不可用：HTTP ${download.status}`);
const bytes = Buffer.from(await download.arrayBuffer());
if (bytes.length !== manifest.size || createHash('sha256').update(bytes).digest('hex') !== expected)
  throw Error('原安装包没有通过 SHA-256 校验');
if (process.argv.includes('--check-only')) {
  console.log(`续接校验通过：${version}，源提交 ${source}，SHA-256 ${expected}`);
  process.exit(0);
}
await mkdir('dist', { recursive: true });
await writeFile(`dist/${name}`, bytes);
await writeFile(`dist/${name}.sha256`, `${expected}  ${name}\n`);
await writeFile(`dist/Sleepy-Doll-${version}-release.json`, JSON.stringify(manifest, null, 2) + '\n');
console.log(`已恢复经检查与公开下载校验的原包：${version}，源提交 ${source}，SHA-256 ${expected}`);
