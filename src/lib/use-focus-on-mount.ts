import { useEffect, useRef } from 'react'

export function useFocusOnMount<Element extends HTMLElement>() {
  const elementRef = useRef<Element>(null)
  useEffect(() => {
    elementRef.current?.focus()
  }, [])
  return elementRef
}

export function useSelectWhen(active: boolean) {
  const inputRef = useRef<HTMLInputElement>(null)
  useEffect(() => {
    if (active) {
      inputRef.current?.focus()
      inputRef.current?.select()
    }
  }, [active])
  return inputRef
}
