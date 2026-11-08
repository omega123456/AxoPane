import type { PlatformOs } from '@/lib/keymap'
import type { CommandId, LayoutConfig } from '@/lib/types/ipc'
import type { DraftState, DraftUpdater } from './settings-draft'

export const SETTINGS_ICON_CLASS = 'size-4'

export type SectionProps = {
  draft: DraftState
  updateDraft: DraftUpdater
  updateLayout: <Key extends keyof LayoutConfig>(key: Key, value: LayoutConfig[Key]) => void
  search: string
  onSearchChange: (search: string) => void
  capturing: CommandId | null
  onCapture: (commandId: CommandId) => void
  conflicts: Map<CommandId, string[]>
  platform: PlatformOs
}
