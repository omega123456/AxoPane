import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { vi } from 'vitest'
import { TreeFoldRow } from '@/components/tree/TreeFoldRow'

describe('TreeFoldRow', () => {
  it('names a level of folded siblings and reveals it on click', async () => {
    const user = userEvent.setup()
    const onReveal = vi.fn()
    render(<TreeFoldRow kind="siblings" count={12} depth={2} onReveal={onReveal} />)

    const item = screen.getByRole('treeitem')
    expect(item).toHaveAttribute('aria-expanded', 'false')
    expect(item.style.paddingLeft).toBe('40px')

    await user.click(screen.getByRole('button', { name: '12 other folders' }))
    expect(onReveal).toHaveBeenCalledTimes(1)
  })

  it('uses the singular for one folded folder', () => {
    render(<TreeFoldRow kind="siblings" count={1} depth={1} onReveal={vi.fn()} />)
    expect(screen.getByRole('button', { name: '1 other folder' })).toBeInTheDocument()
  })

  it('lists the hidden levels of a deep path in its tooltip', async () => {
    const user = userEvent.setup()
    const onReveal = vi.fn()
    render(
      <TreeFoldRow
        kind="levels"
        hiddenNames={['Projects', 'AxoPane', 'src-tauri', 'target']}
        depth={9}
        onReveal={onReveal}
      />,
    )

    const button = screen.getByRole('button', { name: '4 levels hidden' })
    expect(button).toHaveAttribute('title', 'Projects › AxoPane › src-tauri › target')
    expect(screen.getByRole('treeitem').style.paddingLeft).toBe('72px')

    await user.click(button)
    expect(onReveal).toHaveBeenCalledTimes(1)
  })
})
