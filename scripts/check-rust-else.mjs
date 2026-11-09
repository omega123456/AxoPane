import { readdirSync, readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import path from 'node:path'
import { findIfElseLines } from './rust-else-scanner.mjs'

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const rustSourceRoot = path.join(repositoryRoot, 'src-tauri', 'src')
const RUST_EXTENSION = '.rs'
const SLASH = '/'
const FAILURE_EXIT_CODE = 1
const ELSE_VIOLATION_MESSAGE =
  'else outside let-else; use guard clauses, a lookup table, or a named function'

function listRustFiles(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((directoryEntry) =>
    rustFilesUnder(directory, directoryEntry),
  )
}

function rustFilesUnder(directory, directoryEntry) {
  const entryPath = path.join(directory, directoryEntry.name)
  if (directoryEntry.isDirectory()) {
    return listRustFiles(entryPath)
  }
  if (entryPath.endsWith(RUST_EXTENSION)) {
    return [entryPath]
  }
  return []
}

function violationsInFile(filePath) {
  const relativePath = path.relative(repositoryRoot, filePath).split(path.sep).join(SLASH)
  return findIfElseLines(readFileSync(filePath, 'utf8')).map(
    (line) => `${relativePath}:${line}: ${ELSE_VIOLATION_MESSAGE}`,
  )
}

function main() {
  const violations = listRustFiles(rustSourceRoot).flatMap(violationsInFile)
  if (violations.length === 0) {
    return
  }
  for (const violation of violations) {
    console.error(violation)
  }
  console.error(`${violations.length} Rust else violation(s) found`)
  process.exitCode = FAILURE_EXIT_CODE
}

main()
