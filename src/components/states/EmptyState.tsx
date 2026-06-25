import { FolderOpenIcon } from '@/components/icons'

export function EmptyState() {
  return (
    <div
      role="status"
      aria-label="Empty folder"
      className="flex flex-1 flex-col items-center justify-center gap-2 px-6 py-10 text-center"
    >
      <FolderOpenIcon className="h-8 w-8 text-light-text-faint dark:text-dark-text-faint" />
      <p className="text-row text-light-text-muted dark:text-dark-text-muted">
        This folder is empty
      </p>
    </div>
  )
}
