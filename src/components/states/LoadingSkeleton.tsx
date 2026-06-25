const skeletonWidths = [
  'w-48',
  'w-40',
  'w-56',
  'w-44',
  'w-52',
  'w-36',
  'w-60',
  'w-44',
  'w-48',
  'w-40',
]

export function LoadingSkeleton() {
  return (
    <div aria-label="Loading folder" role="status" className="flex flex-col gap-2 px-3 py-2">
      {skeletonWidths.map((width, index) => (
        <div key={`${width}-${index}`} className="flex h-row items-center gap-3">
          <div className="h-4 w-4 shrink-0 animate-pulse rounded-tab bg-light-skeleton-strong dark:bg-dark-skeleton-strong" />
          <div
            className={`h-3 ${width} animate-pulse rounded-tab bg-light-skeleton dark:bg-dark-skeleton`}
          />
        </div>
      ))}
    </div>
  )
}
