/// <reference types="node" />
import { fileURLToPath } from 'node:url'
import path from 'node:path'
import { Linter } from 'eslint'
import tseslint from 'typescript-eslint'
import { localRulesPlugin } from '../../../scripts/eslint-named-conditions.mjs'

const RULE_ID = 'local/named-conditions'
const virtualFileName = 'named-conditions-fixture.ts'
const typedProjectDir = path.join(path.dirname(fileURLToPath(import.meta.url)), 'named-conditions-project')

type LintTarget = {
  config: Linter.Config[]
  cwd: string
  filename: string
}

const ruleConfig = {
  plugins: { local: localRulesPlugin },
  rules: { [RULE_ID]: 'error' },
} satisfies Linter.Config

const untypedConfig: Linter.Config[] = [
  {
    ...ruleConfig,
    files: ['**/*.ts'],
    languageOptions: { parser: tseslint.parser },
  },
]

const typedConfig: Linter.Config[] = [
  {
    ...ruleConfig,
    files: ['**/*.ts'],
    languageOptions: {
      parser: tseslint.parser,
      parserOptions: {
        projectService: true,
        tsconfigRootDir: typedProjectDir,
      },
    },
  },
]

const untypedTarget: LintTarget = {
  config: untypedConfig,
  cwd: process.cwd(),
  filename: virtualFileName,
}

const typedTarget: LintTarget = {
  config: typedConfig,
  cwd: typedProjectDir,
  filename: path.join(typedProjectDir, virtualFileName),
}

function reportedLines(source: string, target: LintTarget = untypedTarget) {
  const linter = new Linter({ configType: 'flat', cwd: target.cwd })
  const messages = linter.verify(source, target.config, { filename: target.filename })
  return messages.map((message) => {
    expect(message.ruleId).toBe(RULE_ID)
    return message.line
  })
}

function lines(...sourceLines: string[]) {
  return sourceLines.join('\n')
}

describe('named-conditions rule', () => {
  it('reports compound logical tests in if, while and ternary positions', () => {
    const source = lines(
      'declare const open: boolean, menu: object | null, count: number',
      'if (!open || !menu) {}',
      'while (count > 0 && open) {}',
      'const label = !open && count > 1 ? "many" : "few"',
    )

    expect(reportedLines(source)).toEqual([2, 3, 4])
  })

  it('allows one operator joining two plain references', () => {
    const source = lines(
      'declare const hasMore: boolean, reachedEnd: boolean',
      'declare const event: { ctrlKey: boolean; metaKey: boolean }',
      'if (hasMore && reachedEnd) {}',
      'if (event.ctrlKey || event.metaKey) {}',
      'const value = hasMore ?? reachedEnd ? 1 : 0',
    )

    expect(reportedLines(source)).toEqual([])
  })

  it('treats computed members as more than a plain reference', () => {
    const source = lines(
      'declare const items: boolean[], fallback: boolean',
      'declare const pane: { entries?: { length: number } }',
      'if (items[0] || fallback) {}',
      'if (pane.entries!.length > 0) {}',
    )

    expect(reportedLines(source)).toEqual([3, 4])
  })

  it('reports comparisons against a call result or a member chain', () => {
    const source = lines(
      'declare function platform(): string',
      'declare const pane: { tabs: { length: number } }',
      'declare const list: { length: number }',
      'if (platform() === "macos") {}',
      'if (pane.tabs.length > 0) {}',
      'if (list.length > 0) {}',
    )

    expect(reportedLines(source)).toEqual([4, 5])
  })

  it('reports a negated chained call but not a negated named predicate', () => {
    const source = lines(
      'declare function isReady(): boolean',
      'declare const seen: Set<string>',
      'declare const sessions: Record<string, { apply(): boolean }>',
      'if (!isReady()) {}',
      'if (!seen.has("a")) {}',
      'if (!sessions.left.apply()) {}',
    )

    expect(reportedLines(source)).toEqual([6])
  })

  it('treats members of this as one level shallower', () => {
    const source = lines(
      'class Session {',
      '  pages = new Set<number>()',
      '  total = 0',
      '  check(index: number) {',
      '    if (this.total === 0) {}',
      '    if (!this.pages.has(index)) {}',
      '    if (this.pages.size > 1) {}',
      '  }',
      '}',
    )

    expect(reportedLines(source)).toEqual([])
  })

  it('reports compound return values outside named predicates', () => {
    const source = lines(
      'declare const ready: boolean, count: number',
      'function visibleState() { return !ready || count === 0 }',
      'function isVisible() { return !ready || count === 0 }',
      'const hasRows = () => { return count > 0 && ready }',
      'const report = { canShow() { return count > 0 && ready } }',
      'function bare() { return }',
    )

    expect(reportedLines(source)).toEqual([2])
  })

  it('reports compound returns inside anonymous callbacks', () => {
    const source = lines(
      'declare const items: { done: boolean; count: number }[]',
      'items.some(function (item) { return item.done && item.count > 0 })',
    )

    expect(reportedLines(source)).toEqual([2])
  })

  it('exempts boolean-returning named functions and non-boolean returns when typed', () => {
    const source = lines(
      'declare const ready: boolean, count: number, name: string | undefined',
      'export function pathsMatch() { return ready && count > 0 }',
      'export function labelFor() { return name || "untitled" }',
      'export const handler = () => { return name !== undefined && count > 0 ? 1 : 0 }',
      'export const anonymous = [1].map(function () { return ready && count > 0 })',
    )

    expect(reportedLines(source, typedTarget)).toEqual([4, 5])
  })
})
