import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { TextAreaField } from '@/components/controls'

describe('TextAreaField', () => {
  it('renders its copy and emits textarea edits', () => {
    const onChange = vi.fn()

    render(
      <TextAreaField
        label="Exclude globs"
        description="One pattern per line."
        value=""
        onChange={onChange}
      />,
    )

    fireEvent.change(screen.getByLabelText('Exclude globs'), {
      target: { value: '*.tmp' },
    })

    expect(onChange).toHaveBeenCalledWith('*.tmp')
  })
})
