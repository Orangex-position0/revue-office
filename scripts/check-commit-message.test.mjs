import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { spawnSync } from 'node:child_process'
import { fileURLToPath, URL } from 'node:url'
import process from 'node:process'

const checker = fileURLToPath(
  new URL('./check-commit-message.mjs', import.meta.url),
)

for (const [message, valid] of [
  ['chore: migrate Git hooks to prek', true],
  ['feat(user-auth)!: change login\r\n\r\nBREAKING CHANGE: new API', true],
  ['# comment\n\nfix(api): handle errors', true],
  ["Merge branch 'main'", true],
  ['Revert "feat: add export"', true],
  ['update code', false],
  ['unknown: add export', false],
  ['fix(user_auth): handle errors', false],
  ['fix: ', false],
  ['', false],
]) {
  test(`valid=${valid}: ${JSON.stringify(message)}`, () => {
    const dir = mkdtempSync(join(tmpdir(), 'revue-commit-'))
    try {
      const file = join(dir, 'COMMIT_EDITMSG')
      writeFileSync(file, message)
      const result = spawnSync(process.execPath, [checker, file], {
        encoding: 'utf8',
      })
      assert.equal(result.status, valid ? 0 : 1, result.stderr)
    } finally {
      rmSync(dir, { recursive: true, force: true })
    }
  })
}
