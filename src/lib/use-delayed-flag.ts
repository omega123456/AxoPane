import { useEffect, useState } from 'react'

/**
 * Debounces a boolean toward `true`: returns `false` until `value` has stayed
 * `true` continuously for `delayMs`. Turning `false` is reflected immediately.
 *
 * Used to suppress the loading skeleton on fast folder loads — it only appears
 * when loading genuinely takes longer than the delay, avoiding a jarring flash.
 */
export function useDelayedFlag(value: boolean, delayMs: number) {
  const [elapsed, setElapsed] = useState(false)

  useEffect(() => {
    if (!value) {
      return
    }

    const timer = setTimeout(() => setElapsed(true), delayMs)
    return () => {
      clearTimeout(timer)
      // Reset for the next `true` period so a previously elapsed timer can't
      // leak across into a fresh, fast load.
      setElapsed(false)
    }
  }, [value, delayMs])

  // Gate on `value` directly so clearing it reads as `false` on the same render,
  // before the effect cleanup runs.
  return value && elapsed
}
