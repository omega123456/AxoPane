import { FolderGlyph } from '@/components/icons'
import { StatePanel } from './StatePanel'

export function EmptyState({ onNewFolder }: { onNewFolder?: () => void }) {
  return (
    <StatePanel
      role="status"
      label="Empty folder"
      icon={<FolderGlyph className="size-6" />}
      title="This folder is empty"
      message={onNewFolder ? 'Drag files here, or create a folder.' : 'Nothing is here.'}
      action={onNewFolder ? { label: 'New folder', onClick: onNewFolder } : undefined}
    />
  )
}
