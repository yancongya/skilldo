#!/usr/bin/env node
import fs from 'node:fs'
import path from 'node:path'
import process from 'node:process'
import { execFileSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

export function nextPatchVersion(version) {
  const match = String(version).match(/^(\d+)\.(\d+)\.(\d+)$/)
  if (!match) throw new Error(`只支持稳定版语义版本：${version}`)
  return `${match[1]}.${match[2]}.${Number(match[3]) + 1}`
}

export function isReleaseWorthyPath(file) {
  return !(
    file === '.gitignore' ||
    file === 'featured-skills.json' ||
    file.endsWith('.md') ||
    file.startsWith('docs/') ||
    file.startsWith('.github/') ||
    file.startsWith('.workbuddy/')
  )
}

export function promoteChangelog(text, version, date, fallbackSubjects = []) {
  const lines = text.split(/\r?\n/)
  const unreleased = lines.findIndex((line) => line.trim() === '## [Unreleased]')
  if (unreleased < 0) throw new Error('CHANGELOG.md 缺少 ## [Unreleased]')
  const nextHeader = lines.findIndex((line, index) => index > unreleased && /^##\s+/.test(line))
  if (nextHeader < 0) throw new Error('CHANGELOG.md 缺少历史版本段落')

  let body = lines.slice(unreleased + 1, nextHeader).join('\n').trim()
  if (!body) {
    const subjects = fallbackSubjects.length ? fallbackSubjects : ['包含 main 分支自上个版本以来的功能改进与问题修复。']
    body = `### 更新\n${subjects.map((subject) => `- ${subject}`).join('\n')}`
  }

  const before = lines.slice(0, unreleased + 1).join('\n')
  const after = lines.slice(nextHeader).join('\n')
  return `${before}\n\n## [${version}] - ${date}\n\n${body}\n\n${after}`.replace(/\n{3,}/g, '\n\n')
}

function git(root, args) {
  return execFileSync('git', args, { cwd: root, encoding: 'utf8' }).trim()
}

function main() {
  const root = process.cwd()
  const [command, baseTag] = process.argv.slice(2)
  if (!['should-release', 'prepare'].includes(command) || !baseTag) {
    console.error('Usage: node scripts/prepare-auto-release.mjs <should-release|prepare> <base-tag>')
    process.exit(2)
  }

  const changedFiles = git(root, ['diff', '--name-only', `${baseTag}..HEAD`]).split('\n').filter(Boolean)
  const worthy = changedFiles.filter(isReleaseWorthyPath)
  if (command === 'should-release') {
    process.stdout.write(worthy.length ? 'true\n' : 'false\n')
    return
  }
  if (!worthy.length) throw new Error(`${baseTag} 之后只有文档或内部维护变更，无需发布`)

  const packagePath = path.join(root, 'package.json')
  const packageJson = JSON.parse(fs.readFileSync(packagePath, 'utf8'))
  const version = nextPatchVersion(packageJson.version)
  execFileSync(process.execPath, ['scripts/version.mjs', 'set', version], { cwd: root, stdio: 'inherit' })

  const subjects = git(root, ['log', '--format=%s', `${baseTag}..HEAD`])
    .split('\n')
    .filter((subject) => subject && !subject.includes('[skip release]'))
  const date = new Date().toISOString().slice(0, 10)
  const changelogPath = path.join(root, 'CHANGELOG.md')
  const changelog = fs.readFileSync(changelogPath, 'utf8')
  fs.writeFileSync(changelogPath, promoteChangelog(changelog, version, date, subjects), 'utf8')
  process.stdout.write(`v${version}\n`)
}

if (fileURLToPath(import.meta.url) === path.resolve(process.argv[1] || '')) {
  try {
    main()
  } catch (error) {
    console.error(error?.stack || String(error))
    process.exit(1)
  }
}
