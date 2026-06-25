import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { RangeSlider } from '@/components/controls'

describe('RangeSlider', () => {
  it('renders a value readout and emits numeric changes', () => {
    const onChange = vi.fn()

    render(
      <RangeSlider
        ariaLabel="Queue opacity"
        min={0}
        max={100}
        value={42}
        valueLabel="42%"
        onChange={onChange}
      />,
    )

    expect(screen.getByText('42%')).toBeInTheDocument()

    fireEvent.change(screen.getByLabelText('Queue opacity'), { target: { value: '57' } })

    expect(onChange).toHaveBeenCalledWith(57)
  })
})
