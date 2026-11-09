import { createRef } from 'react'
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

  it('forwards key presses, the placeholder and its ref, with a focus ring when editable', () => {
    const onKeyDown = vi.fn()
    const ref = createRef<HTMLInputElement>()

    render(
      <FolderInput
        ref={ref}
        ariaLabel="Folder to exclude"
        value=""
        placeholder="/path/to/folder"
        onKeyDown={onKeyDown}
        className="flex-1"
      />,
    )

    const input = screen.getByLabelText('Folder to exclude')
    fireEvent.keyDown(input, { key: 'Enter' })

    expect(onKeyDown).toHaveBeenCalledWith(expect.objectContaining({ key: 'Enter' }))
    expect(ref.current).toBe(input)
    expect(input).toHaveAttribute('placeholder', '/path/to/folder')
    expect(input.parentElement).toHaveClass('focus-within:ring-2', 'flex-1')
  })

  it('draws no focus ring around a read-only path', () => {
    render(<FolderInput ariaLabel="Cache folder" value="C:\\Cache" readOnly />)

    expect(screen.getByLabelText('Cache folder').parentElement).not.toHaveClass(
      'focus-within:ring-2',
    )
  })
})
