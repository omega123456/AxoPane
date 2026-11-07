/**
 * Text with matched ranges emphasised. `ranges` are sorted `[start, end)`
 * UTF-16 offsets (JS string indices), as the search service reports them.
 */
export function HighlightedText({ text, ranges }: { text: string; ranges: [number, number][] }) {
  const parts: { text: string; match: boolean }[] = []
  let cursor = 0
  for (const [start, end] of ranges) {
    if (start < cursor || end > text.length) {
      continue
    }
    if (start > cursor) {
      parts.push({ text: text.slice(cursor, start), match: false })
    }
    parts.push({ text: text.slice(start, end), match: true })
    cursor = end
  }
  if (cursor < text.length) {
    parts.push({ text: text.slice(cursor), match: false })
  }
  return (
    <>
      {parts.map((part, index) =>
        part.match ? (
          <mark
            key={index}
            className="bg-transparent font-semibold text-light-accent dark:text-dark-accent"
          >
            {part.text}
          </mark>
        ) : (
          <span key={index}>{part.text}</span>
        ),
      )}
    </>
  )
}
