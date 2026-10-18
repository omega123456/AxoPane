// Name-bar widths, as fractions of the name column.
const skeletonWidths = [
  'w-7/10',
  'w-11/20',
  'w-4/5',
  'w-9/20',
  'w-13/20',
  'w-1/2',
  'w-3/4',
  'w-3/5',
  'w-17/20',
  'w-2/5',
]

/** Static placeholder rows (no shimmer, so an idle pane costs nothing). */
export function LoadingSkeleton() {
  return (
    <div aria-label="Loading folder" role="status" className="grid gap-2.5 p-3">
      {skeletonWidths.map((width) => (
        <div key={width} className="flex items-center gap-2.5">
          <div className="size-4 shrink-0 rounded-control bg-light-skeleton dark:bg-dark-skeleton" />
          <div className="min-w-0 flex-1">
            <div className={`h-2.5 ${width} rounded-bar bg-light-skeleton dark:bg-dark-skeleton`} />
          </div>
          <div className="h-2.5 w-12 shrink-0 rounded-bar bg-light-skeleton dark:bg-dark-skeleton" />
        </div>
      ))}
    </div>
  )
}
