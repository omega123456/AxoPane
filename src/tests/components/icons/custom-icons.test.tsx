import { render } from '@testing-library/react'
import { DualPaneIcon, SinglePaneIcon } from '@/components/icons/custom-icons'

describe('custom icons', () => {
  it('renders the single-pane glyph', () => {
    const { container } = render(<SinglePaneIcon className="h-4 w-4" />)
    expect(container.querySelector('svg')).toBeInTheDocument()
    expect(container.querySelectorAll('rect')).toHaveLength(1)
  })

  it('renders the dual-pane glyph with two rects', () => {
    const { container } = render(<DualPaneIcon className="h-4 w-4" />)
    expect(container.querySelectorAll('rect')).toHaveLength(2)
  })
})
