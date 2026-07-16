import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { ContextMenuRow } from '@/components/menus/ContextMenuRow'
import { noopContextAction } from '@/lib/context-menu/context-menu-actions'

describe('ContextMenuRow', () => {
  it('renders a native data-url icon when available', () => {
    render(
      <ContextMenuRow
        item={{
          id: 'native-open-terminal',
          kind: 'action',
          label: 'Open in Fixture Terminal',
          owner: 'native',
          icon: {
            kind: 'native',
            dataUrl: 'data:image/png;base64,RkFLRQ==',
            alt: 'Fixture icon',
          },
          action: noopContextAction('native-open-terminal'),
        }}
        active={false}
        onPointerEnter={() => {}}
        onActivate={() => {}}
      />,
    )

    const icon = screen.getByRole('img', { name: 'Fixture icon' })
    expect(icon).toHaveAttribute('src', 'data:image/png;base64,RkFLRQ==')
  })

  it('falls back cleanly when a native icon fails to load', () => {
    render(
      <ContextMenuRow
        item={{
          id: 'native-broken-icon',
          kind: 'action',
          label: 'Broken native icon',
          owner: 'native',
          icon: {
            kind: 'native',
            dataUrl: 'data:image/png;base64,not-a-real-image',
            alt: 'Broken fixture icon',
          },
          action: noopContextAction('native-broken-icon'),
        }}
        active={false}
        onPointerEnter={() => {}}
        onActivate={() => {}}
      />,
    )

    const icon = screen.getByRole('img', { name: 'Broken fixture icon' })
    fireEvent.error(icon)
    expect(screen.queryByRole('img', { name: 'Broken fixture icon' })).not.toBeInTheDocument()
    expect(screen.getByRole('menuitem', { name: 'Broken native icon' })).toBeInTheDocument()
  })

  it('renders movement icons and preserves disabled treatment', () => {
    render(
      <ContextMenuRow
        item={{
          id: 'move-left',
          kind: 'action',
          label: 'Move tab left',
          owner: 'app',
          disabled: true,
          icon: { kind: 'app', name: 'arrow-left' },
          action: noopContextAction('move-left'),
        }}
        active={false}
        onPointerEnter={() => {}}
        onActivate={() => {}}
      />,
    )

    expect(screen.getByRole('menuitem', { name: 'Move tab left' })).toBeDisabled()
  })
})
