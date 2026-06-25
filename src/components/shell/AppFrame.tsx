import type { ReactNode } from 'react'

type AppFrameProps = {
  children: ReactNode
  commandBar: ReactNode
  statusBar: ReactNode
  overlay?: ReactNode
}

export function AppFrame({ children, commandBar, statusBar, overlay }: AppFrameProps) {
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <section className="relative flex min-h-0 flex-1 flex-col overflow-hidden bg-light-window dark:bg-dark-window">
        {commandBar}
        <div className="flex min-h-0 flex-1">{children}</div>
        {statusBar}
        {overlay}
      </section>
    </div>
  )
}
