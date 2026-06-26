/**
 * Run the Rust coverage gate via the `gm-llvm-cov` cargo alias.
 *
 * Resolves the rustup-managed llvm-cov / llvm-profdata so cargo-llvm-cov works
 * regardless of which `cargo` binary is first on PATH, then runs the alias from
 * the repository root (where `.cargo/config.toml` lives).
 *
 * cargo-llvm-cov exits silently (code 1) when a `--fail-under-*` threshold is
 * not met. This wrapper parses the thresholds out of the alias and the actual
 * percentages out of the printed TOTAL row, then prints exactly which gates
 * failed and by how much.
 */
import { execFileSync, spawn } from 'node:child_process'
import { existsSync, readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import path from 'node:path'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')

function tryReadCommand(command, args) {
  try {
    return execFileSync(command, args, {
      cwd: root,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    }).trim()
  } catch {
    return null
  }
}

function ensureRustCoverageTools(env) {
  if (env.LLVM_COV && env.LLVM_PROFDATA) {
    return env
  }

  const rustupHome =
    env.RUSTUP_HOME ??
    tryReadCommand('rustup', ['show', 'home']) ??
    path.join(env.USERPROFILE ?? env.HOME ?? '', '.rustup')
  const toolchain = tryReadCommand('rustup', ['show', 'active-toolchain'])?.split(/\s+/)[0]
  const host = tryReadCommand('rustc', ['-vV'])
    ?.split('\n')
    .find((line) => line.startsWith('host: '))
    ?.replace('host: ', '')
    .trim()

  if (!rustupHome || !toolchain || !host) {
    return env
  }

  const llvmBinDir = path.join(rustupHome, 'toolchains', toolchain, 'lib', 'rustlib', host, 'bin')
  const ext = process.platform === 'win32' ? '.exe' : ''
  const llvmCov = path.join(llvmBinDir, `llvm-cov${ext}`)
  const llvmProfdata = path.join(llvmBinDir, `llvm-profdata${ext}`)

  if (!existsSync(llvmCov) || !existsSync(llvmProfdata)) {
    return env
  }

  return {
    ...env,
    LLVM_COV: env.LLVM_COV ?? llvmCov,
    LLVM_PROFDATA: env.LLVM_PROFDATA ?? llvmProfdata,
  }
}

/** Read the configured `--fail-under-*` thresholds from the gm-llvm-cov alias. */
function readThresholds() {
  const config = readFileSync(path.join(root, '.cargo', 'config.toml'), 'utf8')
  const read = (flag) => {
    const match = config.match(new RegExp(`--fail-under-${flag}\\s+(\\d+(?:\\.\\d+)?)`))
    return match ? Number(match[1]) : null
  }
  return { lines: read('lines'), functions: read('functions'), regions: read('regions') }
}

/**
 * Parse the percentages from the cargo-llvm-cov TOTAL row. Column order is
 * Regions, Functions, Lines (each preceded by missed counts), matching the
 * cargo-llvm-cov text report header.
 */
function parseTotals(output) {
  const total = output.split('\n').find((line) => line.trimStart().startsWith('TOTAL'))
  if (!total) {
    return null
  }
  const percents = [...total.matchAll(/(\d+(?:\.\d+)?)%/g)].map((m) => Number(m[1]))
  if (percents.length < 3) {
    return null
  }
  const [regions, functions, lines] = percents
  return { regions, functions, lines }
}

function reportFailures(thresholds, totals) {
  const order = ['lines', 'functions', 'regions']
  const failures = order
    .filter((metric) => thresholds[metric] != null && totals[metric] < thresholds[metric])
    .map((metric) => {
      const actual = totals[metric]
      const required = thresholds[metric]
      const deficit = (required - actual).toFixed(2)
      return `  ${metric.padEnd(9)} ${actual.toFixed(2)}% < ${required}%  (need +${deficit})`
    })

  if (failures.length === 0) {
    return
  }

  process.stderr.write('\n✗ Rust coverage gate failed:\n')
  process.stderr.write(`${failures.join('\n')}\n`)
}

const env = ensureRustCoverageTools({ ...process.env })

// Use `rustup run stable cargo` to ensure the rustup-managed rustc is used,
// so cargo-llvm-cov resolves llvm-profdata via the correct sysroot regardless
// of which `cargo` binary PATH resolves to (e.g. Homebrew vs rustup shim).
const hasRustup = Boolean(tryReadCommand('rustup', ['--version']))
const [command, commandArgs] = hasRustup
  ? ['rustup', ['run', 'stable', 'cargo', 'gm-llvm-cov']]
  : ['cargo', ['gm-llvm-cov']]

// Capture stdout (where the coverage table is printed) so we can parse the
// TOTAL row, while still streaming it to the terminal in real time. nextest
// progress goes to stderr and is inherited untouched.
const child = spawn(command, commandArgs, {
  cwd: root,
  env,
  stdio: ['inherit', 'pipe', 'inherit'],
  shell: process.platform === 'win32',
})

let captured = ''
child.stdout.on('data', (chunk) => {
  captured += chunk.toString()
  process.stdout.write(chunk)
})

child.on('exit', (code, signal) => {
  const exitCode = signal ? 1 : (code ?? 0)
  if (exitCode !== 0) {
    const totals = parseTotals(captured)
    const thresholds = readThresholds()
    if (totals) {
      reportFailures(thresholds, totals)
    }
  }
  process.exit(exitCode)
})
