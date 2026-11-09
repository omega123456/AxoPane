import type { IndexExclusionOutcome, IndexExclusionOutcomeKind } from '@/lib/types/ipc'
import { basenameFromPath } from '@/stores/panes/pane-paths'

export type ExclusionMessageTone = 'info' | 'warning' | 'danger'

export type ExclusionMessage = {
  tone: ExclusionMessageTone
  text: string
}

const SINGLE_FOLDER = 1
const NO_INDEX = -1

const ADDING_OUTCOMES: ReadonlySet<IndexExclusionOutcomeKind> = new Set([
  'added',
  'mergedChildren',
  'homeWarning',
])

function folderName(path: string) {
  return basenameFromPath(path)
}

function excludedFolderCount(count: number) {
  return count === SINGLE_FOLDER ? 'an excluded folder' : `${count} excluded folders`
}

function info(text: string): ExclusionMessage {
  return { tone: 'info', text }
}

function warning(text: string): ExclusionMessage {
  return { tone: 'warning', text }
}

function danger(text: string): ExclusionMessage {
  return { tone: 'danger', text }
}

const outcomeMessages: Record<
  IndexExclusionOutcomeKind,
  (outcome: IndexExclusionOutcome) => ExclusionMessage | null
> = {
  added: () => null,
  duplicate: (outcome) => info(`${folderName(outcome.path)} is already excluded.`),
  insideExisting: (outcome) =>
    info(
      `Already inside ${folderName(outcome.detail ?? outcome.path)}, which is excluded. Remove that exclusion to index this folder.`,
    ),
  mergedChildren: (outcome) =>
    info(
      `${folderName(outcome.path)} already contained ${excludedFolderCount(outcome.mergedCount ?? SINGLE_FOLDER)}, so the list now shows only ${folderName(outcome.path)}.`,
    ),
  alwaysSkipped: (outcome) => info(`AxoPane always skips ${outcome.path}.`),
  rootRejected: () => danger("You can't exclude the whole disk."),
  homeWarning: () =>
    warning("Your home folder is excluded, so most of your files won't appear in search."),
  notFound: (outcome) => danger(`${outcome.path} doesn't exist.`),
  notAFolder: (outcome) => danger(`${outcome.path} is not a folder.`),
}

function isAdding(outcome: IndexExclusionOutcome) {
  return ADDING_OUTCOMES.has(outcome.outcome)
}

export function addedPaths(outcomes: IndexExclusionOutcome[]) {
  return outcomes.filter(isAdding).map((outcome) => outcome.path)
}

function addSummary(added: string[]): ExclusionMessage | null {
  if (added.length === 0) {
    return null
  }
  if (added.length === SINGLE_FOLDER) {
    return info(`Added ${folderName(added[0] ?? '')}.`)
  }
  return info(`Added ${added.length} folders.`)
}

function outcomeMessage(outcome: IndexExclusionOutcome) {
  return outcomeMessages[outcome.outcome](outcome)
}

function isMessage(message: ExclusionMessage | null): message is ExclusionMessage {
  return message !== null
}

function isFirstWithText(message: ExclusionMessage, index: number, messages: ExclusionMessage[]) {
  return messages.findIndex((candidate) => candidate.text === message.text) === index
}

export function messagesForOutcomes(outcomes: IndexExclusionOutcome[]): ExclusionMessage[] {
  const summary = addSummary(addedPaths(outcomes))
  const details = outcomes.map(outcomeMessage).filter(isMessage)
  return [summary, ...details].filter(isMessage).filter(isFirstWithText)
}

export function removalMessages(path: string): ExclusionMessage[] {
  return [info(`Removed ${folderName(path)}. Updating the index in the background.`)]
}

function duplicateTarget(outcomes: IndexExclusionOutcome[]) {
  const duplicate = outcomes.find((outcome) => outcome.outcome === 'duplicate')
  return duplicate?.detail ?? duplicate?.path ?? null
}

export function focusAfterAdd(outcomes: IndexExclusionOutcome[]): string | null {
  const lastAdded = addedPaths(outcomes).at(-1)
  return lastAdded ?? duplicateTarget(outcomes)
}

export function focusAfterRemove(exclusionsBefore: string[], removed: string): string | null {
  const removedIndex = exclusionsBefore.indexOf(removed)
  if (removedIndex === NO_INDEX) {
    return null
  }
  const next = exclusionsBefore[removedIndex + 1]
  return next ?? exclusionsBefore[removedIndex - 1] ?? null
}
