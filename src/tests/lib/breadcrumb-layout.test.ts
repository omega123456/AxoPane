import { describe, expect, it } from 'vitest'
import { computeBreadcrumbLayout } from '@/lib/breadcrumb-layout'

const measure = {
  segment: (label: string) => label.length * 10 + 20,
  currentSegment: (label: string) => label.length * 10 + 10,
  collapseMarker: () => 18,
}

describe('computeBreadcrumbLayout', () => {
  it('returns full labels when every segment already fits', () => {
    const layout = computeBreadcrumbLayout({
      segments: [
        { label: 'C:', path: 'C:\\' },
        { label: 'Users', path: 'C:\\Users' },
        { label: 'Omega', path: 'C:\\Users\\Omega' },
      ],
      availableWidth: 200,
      measure,
    })

    expect(layout).toEqual({
      collapsed: false,
      items: [
        { label: 'C:', fullLabel: 'C:', path: 'C:\\', truncated: false },
        { label: 'Users', fullLabel: 'Users', path: 'C:\\Users', truncated: false },
        {
          label: 'Omega',
          fullLabel: 'Omega',
          path: 'C:\\Users\\Omega',
          truncated: false,
        },
      ],
    })
  })

  it('prefers collapsed trailing crumbs over shortening every label when the full path overflows', () => {
    const layout = computeBreadcrumbLayout({
      segments: [
        { label: 'aaaa', path: 'C:\\aaaa' },
        { label: 'bbbb', path: 'C:\\aaaa\\bbbb' },
        { label: 'cccc', path: 'C:\\aaaa\\bbbb\\cccc' },
      ],
      availableWidth: 140,
      measure,
    })

    expect(layout.collapsed).toBe(true)
    expect(layout.items.map((item) => item.label)).toEqual(['bbbb', 'cccc'])
    expect(layout.items.every((item) => item.truncated)).toBe(false)
  })

  it('still expands surviving crumbs back to full labels after collapsing leading segments', () => {
    const layout = computeBreadcrumbLayout({
      segments: [
        { label: 'aaaa', path: 'C:\\aaaa' },
        { label: 'bbbb', path: 'C:\\aaaa\\bbbb' },
        { label: 'cccc', path: 'C:\\aaaa\\bbbb\\cccc' },
      ],
      availableWidth: 155,
      measure,
    })

    expect(layout.collapsed).toBe(true)
    expect(layout.items.map((item) => item.label)).toEqual(['bbbb', 'cccc'])
  })

  it('drops to the current folder alone before shortening it when that is the clearest fit', () => {
    const layout = computeBreadcrumbLayout({
      segments: [
        { label: '/', path: '/' },
        { label: 'home', path: '/home' },
        { label: 'omega', path: '/home/omega' },
      ],
      availableWidth: 100,
      measure,
    })

    expect(layout.collapsed).toBe(true)
    expect(layout.items.map((item) => item.label)).toEqual(['omega'])
  })

  it('drops leading segments and keeps the current folder fully visible when needed', () => {
    const layout = computeBreadcrumbLayout({
      segments: [
        { label: 'C:', path: 'C:\\' },
        { label: 'Alpha', path: 'C:\\Alpha' },
        { label: 'Bravo', path: 'C:\\Alpha\\Bravo' },
        { label: 'Charlie', path: 'C:\\Alpha\\Bravo\\Charlie' },
      ],
      availableWidth: 100,
      measure,
    })

    expect(layout.collapsed).toBe(true)
    expect(layout.items).toEqual([
      {
        label: 'Charlie',
        fullLabel: 'Charlie',
        path: 'C:\\Alpha\\Bravo\\Charlie',
        truncated: false,
      },
    ])
  })

  it('reuses freed space after collapsing instead of pinning surviving crumbs to the floor', () => {
    const layout = computeBreadcrumbLayout({
      segments: [
        { label: 'C:', path: 'C:\\' },
        { label: 'Alpha', path: 'C:\\Alpha' },
        { label: 'Beta', path: 'C:\\Alpha\\Beta' },
        { label: 'Bravo', path: 'C:\\Alpha\\Bravo' },
        { label: 'Charlie', path: 'C:\\Alpha\\Beta\\Bravo\\Charlie' },
      ],
      availableWidth: 158,
      measure,
    })

    expect(layout.collapsed).toBe(true)
    expect(layout.items.map((item) => item.label)).toEqual(['Brav', 'Charlie'])
  })

  it('accounts for the collapse marker width before deciding how many segments to keep', () => {
    const layout = computeBreadcrumbLayout({
      segments: [
        { label: 'C:', path: 'C:\\' },
        { label: 'Alpha', path: 'C:\\Alpha' },
        { label: 'Charlie', path: 'C:\\Alpha\\Charlie' },
      ],
      availableWidth: 109,
      measure,
    })

    expect(layout.collapsed).toBe(true)
    expect(layout.items.map((item) => item.label)).toEqual(['Charlie'])
  })

  it('keeps the collapse marker by truncating the final segment when the full current folder alone is too wide', () => {
    const layout = computeBreadcrumbLayout({
      segments: [
        { label: 'C:', path: 'C:\\' },
        { label: 'Users', path: 'C:\\Users' },
        { label: 'Projects', path: 'C:\\Users\\Projects' },
        {
          label: 'pages-desktop-cookie-conse-b8d88--Banner-section-screenshots-tablet-retry2',
          path: 'C:\\Users\\Projects\\pages-desktop-cookie-conse-b8d88--Banner-section-screenshots-tablet-retry2',
        },
      ],
      availableWidth: 80,
      measure,
    })

    expect(layout.collapsed).toBe(true)
    expect(layout.items).toHaveLength(1)
    expect(layout.items[0]?.label.length).toBeLessThan(
      'pages-desktop-cookie-conse-b8d88--Banner-section-screenshots-tablet-retry2'.length,
    )
  })

  it('falls back to the full layout when width is unavailable', () => {
    const layout = computeBreadcrumbLayout({
      segments: [
        { label: 'C:', path: 'C:\\' },
        { label: 'Users', path: 'C:\\Users' },
      ],
      availableWidth: 0,
      measure,
    })

    expect(layout.collapsed).toBe(false)
    expect(layout.items.map((item) => item.label)).toEqual(['C:', 'Users'])
  })

  it('falls back to the full layout when segment measurement is not finite', () => {
    const layout = computeBreadcrumbLayout({
      segments: [
        { label: 'C:', path: 'C:\\' },
        { label: 'Users', path: 'C:\\Users' },
      ],
      availableWidth: 80,
      measure: {
        segment: () => Number.NaN,
      },
    })

    expect(layout.collapsed).toBe(false)
    expect(layout.items.map((item) => item.label)).toEqual(['C:', 'Users'])
  })
})
