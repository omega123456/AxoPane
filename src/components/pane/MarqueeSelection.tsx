export type MarqueeRect = { left: number; top: number; width: number; height: number }

export function MarqueeSelection({ rect }: { rect: MarqueeRect | null | undefined }) {
  if (!rect) {
    return null
  }
  return (
    <div
      data-testid="marquee-selection"
      className="pointer-events-none absolute border border-light-accent dark:border-dark-accent bg-light-tint dark:bg-dark-tint"
      style={{
        left: `${rect.left}px`,
        top: `${rect.top}px`,
        width: `${rect.width}px`,
        height: `${rect.height}px`,
      }}
    />
  )
}
