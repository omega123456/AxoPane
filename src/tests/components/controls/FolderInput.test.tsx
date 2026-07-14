import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { FolderInput } from '@/components/controls'

describe('FolderInput', () => {
  it('shows the current path and forwards edits', () => {
    const onChange = vi.fn()

    render(<FolderInput ariaLabel="Cache folder" value="C:\\Cache" onChange={onChange} />)

    const input = screen.getByLabelText('Cache folder')
    fireEvent.change(input, { target: { value: 'D:\\Temp' } })

    expect(onChange).toHaveBeenCalledWith('D:\\Temp')
  })
})
