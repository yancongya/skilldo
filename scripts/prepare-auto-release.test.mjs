import test from 'node:test'
import assert from 'node:assert/strict'
import { isReleaseWorthyPath, nextPatchVersion, promoteChangelog } from './prepare-auto-release.mjs'

test('increments stable patch versions', () => {
  assert.equal(nextPatchVersion('0.7.1'), '0.7.2')
  assert.throws(() => nextPatchVersion('0.7.1-beta.1'))
})

test('skips documentation and generated catalog changes', () => {
  assert.equal(isReleaseWorthyPath('README.md'), false)
  assert.equal(isReleaseWorthyPath('docs/README.zh.md'), false)
  assert.equal(isReleaseWorthyPath('featured-skills.json'), false)
  assert.equal(isReleaseWorthyPath('src/App.tsx'), true)
})

test('promotes Chinese unreleased notes to a version section', () => {
  const input = '# Changelog\n\n## [Unreleased]\n\n### Added\n- 新功能\n\n## [0.7.1] - 2026-09-01\n\n- 旧版本\n'
  const output = promoteChangelog(input, '0.7.2', '2026-09-01')
  assert.match(output, /## \[Unreleased\]\n\n## \[0\.7\.2\] - 2026-09-01/)
  assert.match(output, /### Added\n- 新功能/)
})
