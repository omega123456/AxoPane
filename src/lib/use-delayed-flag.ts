import { useEffect, useState } from 'react'

export function useDelayedFlag(value: boolean, delayMs: number) {
  const [elapsed, setElapsed] = useState(false)

  useEffect(() => {
    if (!value) {
      return
    }

    const timer = setTimeout(() => setElapsed(true), delayMs)
    return () => {
      clearTimeout(timer)
      setElapsed(false)
    }
  }, [value, delayMs])

  return value && elapsed
}
