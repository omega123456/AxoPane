import { render } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { HighlightedText } from '@/components/search/HighlightedText'

function marks(container: HTMLElement) {
  return Array.from(container.querySelectorAll('mark')).map((mark) => mark.textContent)
}

describe('HighlightedText', () => {
  it('emphasises each matched range', () => {
    const { container } = render(
      <HighlightedText
        text="Q3 report.docx"
        ranges={[
          [0, 2],
          [3, 9],
        ]}
      />,
    )
    expect(marks(container)).toEqual(['Q3', 'report'])
    expect(container.textContent).toBe('Q3 report.docx')
  })

  it('uses UTF-16 offsets and ignores overlapping or out-of-range ranges', () => {
    const { container } = render(
      <HighlightedText
        text="😀 report"
        ranges={[
          [3, 9],
          [4, 6],
          [8, 40],
        ]}
      />,
    )
    expect(marks(container)).toEqual(['report'])
    expect(container.textContent).toBe('😀 report')
  })

  it('renders plain text without ranges', () => {
    const { container } = render(<HighlightedText text="Invoices" ranges={[]} />)
    expect(marks(container)).toEqual([])
    expect(container.textContent).toBe('Invoices')
  })
})
