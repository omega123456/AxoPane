import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { SectionLabel } from '@/components/controls'

describe('SectionLabel', () => {
  it('renders section copy with optional class names', () => {
    render(<SectionLabel className="mb-4">Advanced</SectionLabel>)

    expect(screen.getByText('Advanced')).toHaveClass('mb-4')
  })
})
