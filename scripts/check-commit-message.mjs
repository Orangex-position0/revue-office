import { readFileSync } from 'node:fs'
import process from 'node:process'
import console from 'node:console'

const file = process.argv[2]
if (!file) {
  console.error('Usage: node scripts/check-commit-message.mjs <message-file>')
  process.exit(1)
}

const header = readFileSync(file, 'utf8')
  .split(/\r?\n/)
  .find((line) => line.trim() && !line.startsWith('#'))

const conventional =
  /^(feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert)(\([a-z0-9]+(?:-[a-z0-9]+)*\))?!?: \S.*$/
const gitGenerated = /^(Merge .+|Revert ".+")$/

if (!header || (!conventional.test(header) && !gitGenerated.test(header))) {
  console.error(
    'Expected: type(scope)!: description (scope and ! are optional).',
  )
  console.error(
    'Types: feat, fix, docs, style, refactor, perf, test, build, ci, chore, revert.',
  )
  console.error('Example: chore: migrate Git hooks to prek')
  process.exit(1)
}
