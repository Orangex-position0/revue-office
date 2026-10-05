import { spawnSync } from 'node:child_process'
import { fileURLToPath, URL } from 'node:url'
import process from 'node:process'
import console from 'node:console'

// cargo-todo has no --manifest-path option; run it in the backend directory.
const result = spawnSync('cargo', ['todo'], {
  cwd: fileURLToPath(new URL('../src-tauri/', import.meta.url)),
  stdio: 'inherit',
})

if (result.error) {
  console.error(`Unable to run cargo todo: ${result.error.message}`)
}
process.exit(result.status ?? 1)
