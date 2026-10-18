/** The 3px accent pill marking the current item in a Fluent navigation list. */
export function SelectionPill() {
  return (
    <span
      aria-hidden="true"
      className="absolute inset-y-2 left-0 w-0.75 rounded-full bg-light-accent dark:bg-dark-accent"
    />
  )
}
