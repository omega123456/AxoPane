type BreadcrumbSourceSegment = {
  label: string
  path: string
}

export type LayoutSegment = {
  label: string
  fullLabel: string
  path: string
  truncated: boolean
}

export type BreadcrumbLayout = {
  items: LayoutSegment[]
  collapsed: boolean
}

export type BreadcrumbLayoutMeasure = {
  segment: (label: string) => number
  currentSegment?: (label: string) => number
  collapseMarker?: () => number
}

type ComputeBreadcrumbLayoutArgs = {
  segments: BreadcrumbSourceSegment[]
  availableWidth: number
  measure: BreadcrumbLayoutMeasure
  minChars?: number
}

const DEFAULT_MIN_CHARS = 2
const COLLAPSE_MARKER_LABEL = '…'
// Matches the address row: 6px padding either side of each crumb and a 12px
// chevron between crumbs.
const BREADCRUMB_BUTTON_PADDING_PX = 12
const BREADCRUMB_CHEVRON_WIDTH_PX = 12
const BREADCRUMB_SEGMENT_CHROME_PX = BREADCRUMB_BUTTON_PADDING_PX + BREADCRUMB_CHEVRON_WIDTH_PX
const BREADCRUMB_CURRENT_SEGMENT_CHROME_PX = BREADCRUMB_BUTTON_PADDING_PX
const BREADCRUMB_COLLAPSE_MARKER_CHROME_PX =
  BREADCRUMB_BUTTON_PADDING_PX + BREADCRUMB_CHEVRON_WIDTH_PX

type FittedLabels = string[] | null | 'invalid'
type LayoutAttempt = BreadcrumbLayout | null | 'invalid'

type FitContext = {
  segments: BreadcrumbSourceSegment[]
  floorLengths: number[]
  availableWidth: number
  collapseMarkerWidth: number
  segmentMeasure: (label: string) => number
  currentSegmentMeasure: (label: string) => number
}

function layoutAttempt(
  segments: BreadcrumbSourceSegment[],
  labels: FittedLabels,
  collapsed: boolean,
): LayoutAttempt {
  if (!Array.isArray(labels)) {
    return labels
  }
  return createLayout(segments, labels, collapsed)
}

function fitWithCollapsedPrefix(context: FitContext): LayoutAttempt {
  const { segments, floorLengths, collapseMarkerWidth } = context
  for (let dropped = 1; dropped < segments.length; dropped += 1) {
    const keptSegments = segments.slice(dropped)
    const keptLabels = findShortestFittingLabels({
      segments: keptSegments,
      floorLengths: floorLengths.slice(dropped),
      availableWidth: context.availableWidth - collapseMarkerWidth,
      segmentMeasure: context.segmentMeasure,
      currentSegmentMeasure: context.currentSegmentMeasure,
      keepLastFull: true,
    })
    if (keptLabels !== null) {
      return layoutAttempt(keptSegments, keptLabels, true)
    }
  }
  return null
}

function fitCurrentFolderOnly(context: FitContext): LayoutAttempt {
  const finalSegment = context.segments[context.segments.length - 1]!
  const labels = findShortestFittingLabels({
    segments: [finalSegment],
    floorLengths: [context.floorLengths[context.floorLengths.length - 1]!],
    availableWidth: context.availableWidth - context.collapseMarkerWidth,
    segmentMeasure: context.segmentMeasure,
    currentSegmentMeasure: context.currentSegmentMeasure,
  })
  return layoutAttempt([finalSegment], labels, true)
}

function fitAllShortened(context: FitContext): LayoutAttempt {
  const labels = findShortestFittingLabels({
    segments: context.segments,
    floorLengths: context.floorLengths,
    availableWidth: context.availableWidth,
    segmentMeasure: context.segmentMeasure,
    currentSegmentMeasure: context.currentSegmentMeasure,
  })
  return layoutAttempt(context.segments, labels, false)
}

const shorteningStrategies: ((context: FitContext) => LayoutAttempt)[] = [
  fitWithCollapsedPrefix,
  fitCurrentFolderOnly,
  fitAllShortened,
]

function shortenedLayout(context: FitContext): BreadcrumbLayout {
  for (const strategy of shorteningStrategies) {
    const attempt = strategy(context)
    if (attempt === 'invalid') {
      return createFullLayout(context.segments)
    }
    if (attempt) {
      return attempt
    }
  }
  const finalSegment = context.segments[context.segments.length - 1]!
  return createLayout([finalSegment], [finalSegment.label], false)
}

function hasMeasurableWidth(availableWidth: number) {
  return availableWidth > 0 && Number.isFinite(availableWidth)
}

export function computeBreadcrumbLayout({
  segments,
  availableWidth,
  measure,
  minChars = DEFAULT_MIN_CHARS,
}: ComputeBreadcrumbLayoutArgs): BreadcrumbLayout {
  if (segments.length === 0) {
    return { items: [], collapsed: false }
  }
  if (!hasMeasurableWidth(availableWidth)) {
    return createFullLayout(segments)
  }

  const currentSegmentMeasure = measure.currentSegment ?? measure.segment
  const fullLabels = segments.map((segment) => segment.label)
  const fullWidth = sumLayoutWidths(fullLabels, measure.segment, currentSegmentMeasure)
  if (!Number.isFinite(fullWidth)) {
    return createFullLayout(segments)
  }
  if (fullWidth <= availableWidth) {
    return createLayout(segments, fullLabels, false)
  }

  const collapseMarkerWidth = (
    measure.collapseMarker ?? (() => measure.segment(COLLAPSE_MARKER_LABEL))
  )()
  if (!Number.isFinite(collapseMarkerWidth)) {
    return createFullLayout(segments)
  }

  return shortenedLayout({
    segments,
    floorLengths: segments.map((segment) => Math.min(minChars, segment.label.length)),
    availableWidth,
    collapseMarkerWidth,
    segmentMeasure: measure.segment,
    currentSegmentMeasure,
  })
}

export function createBreadcrumbMeasurer(navElement: HTMLElement | null): BreadcrumbLayoutMeasure {
  if (!navElement) {
    return { segment: () => Number.NaN }
  }

  const canvas = document.createElement('canvas')
  const context = canvas.getContext('2d')
  if (!context) {
    return { segment: () => Number.NaN }
  }

  const style = window.getComputedStyle(navElement)
  const font = style.font
  // The current crumb is semibold, so it is measured in the bolder weight.
  const currentFont = `600 ${style.fontSize} ${style.fontFamily}`
  const measure = (label: string, weightFont: string) => {
    if (weightFont) {
      context.font = weightFont
    }
    return context.measureText(label).width
  }

  return {
    segment: (label) => measure(label, font) + BREADCRUMB_SEGMENT_CHROME_PX,
    currentSegment: (label) => measure(label, currentFont) + BREADCRUMB_CURRENT_SEGMENT_CHROME_PX,
    collapseMarker: () =>
      measure(COLLAPSE_MARKER_LABEL, font) + BREADCRUMB_COLLAPSE_MARKER_CHROME_PX,
  }
}

function createFullLayout(segments: BreadcrumbSourceSegment[]): BreadcrumbLayout {
  return createLayout(
    segments,
    segments.map((segment) => segment.label),
    false,
  )
}

function createLayout(
  segments: BreadcrumbSourceSegment[],
  labels: string[],
  collapsed: boolean,
): BreadcrumbLayout {
  return {
    items: segments.map((segment, index) => ({
      label: labels[index]!,
      fullLabel: segment.label,
      path: segment.path,
      truncated: labels[index] !== segment.label,
    })),
    collapsed,
  }
}

function sumLayoutWidths(
  labels: string[],
  segmentMeasure: (label: string) => number,
  currentSegmentMeasure: (label: string) => number,
) {
  return labels.reduce((totalWidth, label, index) => {
    const isLast = index === labels.length - 1
    const nextWidth = isLast ? currentSegmentMeasure(label) : segmentMeasure(label)
    return totalWidth + nextWidth
  }, 0)
}

function findShortestFittingLabels({
  segments,
  floorLengths,
  availableWidth,
  segmentMeasure,
  currentSegmentMeasure,
  keepLastFull = false,
}: {
  segments: BreadcrumbSourceSegment[]
  floorLengths: number[]
  availableWidth: number
  segmentMeasure: (label: string) => number
  currentSegmentMeasure: (label: string) => number
  keepLastFull?: boolean
}): string[] | null | 'invalid' {
  const maxCut = Math.max(
    ...segments.map((segment, index) => {
      const isLockedLast = keepLastFull && index === segments.length - 1
      return isLockedLast ? 0 : segment.label.length - floorLengths[index]!
    }),
  )

  for (let cut = 0; cut <= maxCut; cut += 1) {
    const labels = segments.map((segment, index) => {
      const isLockedLast = keepLastFull && index === segments.length - 1
      if (isLockedLast) {
        return segment.label
      }

      return segment.label.slice(0, Math.max(floorLengths[index]!, segment.label.length - cut))
    })
    const totalWidth = sumLayoutWidths(labels, segmentMeasure, currentSegmentMeasure)
    if (!Number.isFinite(totalWidth)) {
      return 'invalid'
    }
    if (totalWidth <= availableWidth) {
      return expandLabelsToFillAvailableWidth({
        segments,
        labels,
        availableWidth,
        segmentMeasure,
        currentSegmentMeasure,
      })
    }
  }

  return null
}

function expandLabelsToFillAvailableWidth({
  segments,
  labels,
  availableWidth,
  segmentMeasure,
  currentSegmentMeasure,
}: {
  segments: BreadcrumbSourceSegment[]
  labels: string[]
  availableWidth: number
  segmentMeasure: (label: string) => number
  currentSegmentMeasure: (label: string) => number
}) {
  const expandedLabels = [...labels]

  while (true) {
    let changed = false

    for (let index = segments.length - 1; index >= 0; index -= 1) {
      const fullLabel = segments[index]!.label
      const currentLabel = expandedLabels[index]!
      if (currentLabel.length >= fullLabel.length) {
        continue
      }

      const nextLabels = [...expandedLabels]
      nextLabels[index] = fullLabel.slice(0, currentLabel.length + 1)
      const totalWidth = sumLayoutWidths(nextLabels, segmentMeasure, currentSegmentMeasure)
      if (totalWidth <= availableWidth) {
        expandedLabels[index] = nextLabels[index]!
        changed = true
      }
    }

    if (!changed) {
      return expandedLabels
    }
  }
}
