import { render } from '@testing-library/react'
import type { FileCategory } from '@/lib/file-type'
import { EntryIcon } from '@/components/icons/EntryIcon'

function svgFor(name: string, isDir = false) {
  const { container } = render(<EntryIcon entry={{ name, isDir }} />)
  return container.querySelector('svg')
}

describe('EntryIcon', () => {
  const cases: Array<[string, FileCategory]> = [
    ['main.ts', 'code'],
    ['index.html', 'web'],
    ['settings.json', 'config'],
    ['notes.txt', 'text'],
    ['report.pdf', 'office'],
    ['photo.png', 'image'],
    ['song.mp3', 'audio'],
    ['clip.mp4', 'video'],
    ['bundle.zip', 'archive'],
    ['disk.iso', 'disc'],
    ['installer.exe', 'executable'],
    ['font.ttf', 'font'],
    ['data.sqlite', 'database'],
    ['unknown.qwzzz', 'generic'],
  ]

  it.each(cases)('tags %s with data-file-category=%s', (name, category) => {
    const svg = svgFor(name)
    expect(svg).toHaveAttribute('data-file-category', category)
  })

  it('applies the default category color when none is overridden', () => {
    const svg = svgFor('main.ts')
    expect(svg).toHaveClass('text-file-code')
  })

  it('honors a colorClassName override', () => {
    const { container } = render(
      <EntryIcon entry={{ name: 'main.ts', isDir: false }} colorClassName="text-accent-red" />,
    )
    const svg = container.querySelector('svg')
    expect(svg).toHaveClass('text-accent-red')
    expect(svg).not.toHaveClass('text-file-code')
  })

  it('allows an empty colorClassName to inherit the surrounding color', () => {
    const { container } = render(
      <EntryIcon entry={{ name: 'Documents', isDir: true }} colorClassName="" />,
    )
    const svg = container.querySelector('svg')
    expect(svg?.getAttribute('class')).not.toMatch(/text-/)
  })

  it('renders a closed folder with a faint fill', () => {
    const svg = svgFor('Documents', true)
    expect(svg).toHaveAttribute('data-file-category', 'folder')
    expect(svg).toHaveAttribute('fill', 'currentColor')
    expect(svg).toHaveAttribute('fill-opacity', '0.18')
  })

  it('renders an open folder with a stronger fill', () => {
    const { container } = render(
      <EntryIcon entry={{ name: 'Documents', isDir: true }} isOpen />,
    )
    const svg = container.querySelector('svg')
    expect(svg).toHaveAttribute('fill-opacity', '0.26')
  })

  it('uses special-folder glyphs by name', () => {
    const downloads = svgFor('Downloads', true)
    const git = svgFor('.git', true)
    const modules = svgFor('node_modules', true)
    // lucide encodes the glyph identity in its class list (e.g. lucide-folder-down).
    expect(downloads?.getAttribute('class')).toMatch(/folder-down/)
    expect(git?.getAttribute('class')).toMatch(/folder-git/)
    expect(modules?.getAttribute('class')).toMatch(/folder-cog/)
  })

  it('files render without a fill so they stay stroke-only', () => {
    const svg = svgFor('main.ts')
    expect(svg).not.toHaveAttribute('fill', 'currentColor')
  })
})
