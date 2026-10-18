import type { ReactNode } from 'react'

type AppFrameProps = {
  children: ReactNode
  /** App-wide InfoBars (update ready, folder sizes unavailable) above the workspace. */
  infoBars?: ReactNode
  statusBar: ReactNode
  overlay?: ReactNode
}

export function AppFrame({ children, infoBars, statusBar, overlay }: AppFrameProps) {
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <section className="relative flex min-h-0 flex-1 flex-col overflow-hidden bg-light-base dark:bg-dark-base mica:bg-transparent">
        {infoBars ? (
          <div className="flex flex-col gap-gutter px-gutter pt-gutter empty:hidden">
            {infoBars}
          </div>
        ) : null}
        <div className="flex min-h-0 min-w-0 flex-1 overflow-hidden">{children}</div>
        {statusBar}
        {overlay}
      </section>
    </div>
  )
}
