import type { ReactNode } from 'react'
import { PieChartIcon } from '@/components/icons'
import { SettingRow, ToggleSwitch } from '@/components/controls'
import { SETTINGS_ICON_CLASS as iconClass, type SectionProps } from './section-props'

const AUTO_FOLDER_SIZE_TITLE = 'Automatically calculate folder sizes'

export function AutoFolderSizeRow({
  draft,
  updateDraft,
  description,
}: Pick<SectionProps, 'draft' | 'updateDraft'> & { description: ReactNode }) {
  return (
    <SettingRow
      icon={<PieChartIcon className={iconClass} />}
      title={AUTO_FOLDER_SIZE_TITLE}
      description={description}
      control={
        <ToggleSwitch
          label={AUTO_FOLDER_SIZE_TITLE}
          checked={draft.autoFolderSize}
          onChange={(value) => updateDraft((current) => ({ ...current, autoFolderSize: value }))}
        />
      }
    />
  )
}
