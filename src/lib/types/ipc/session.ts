// Mirrors `src-tauri/src/directory_session/model.rs` field-for-field; Rust
// serializes every one of these with `#[serde(rename_all = "camelCase")]`.

import type { DirectoryEntry, SortDirection, SortKey } from './listing'

/** Row page size for every v2 range response. Matches Rust `SESSION_PAGE_SIZE`. */
export const SESSION_PAGE_SIZE = 500

export type SessionId = number
export type NavigationRevision = number
export type WatchRevision = number
export type ViewRevision = number

export type SessionBaseline = {
  sessionId: SessionId
  navigationRevision: NavigationRevision
  watchRevision: WatchRevision
  viewRevision: ViewRevision
}

export type SessionViewParams = {
  sortKey: SortKey
  sortDirection: SortDirection
  filter: string
  showHidden: boolean
  includeItemCounts: boolean
}

export type BeginNavigationRequest = {
  paneId: string
  tabId: string
  path: string
  view: SessionViewParams
}

export type SessionRangePage = {
  pageIndex: number
  entries: DirectoryEntry[]
}

export type BeginNavigationResponse = {
  paneId: string
  tabId: string
  path: string
  baseline: SessionBaseline
  totalRows: number
  pageSize: number
  firstPage: SessionRangePage
}

export type GetSessionRangeRequest = {
  paneId: string
  tabId: string
  baseline: SessionBaseline
  pageIndex: number
}

export type SessionRangeResponse = {
  baseline: SessionBaseline
  totalRows: number
  page: SessionRangePage
}

export type ReviseSessionViewRequest = {
  paneId: string
  tabId: string
  sessionId: SessionId
  view: SessionViewParams
}

export type ReleaseSessionRequest = {
  paneId: string
  tabId: string
  sessionId: SessionId
  navigationRevision: NavigationRevision
}

export type ReleaseSessionResponse = {
  released: boolean
}

/**
 * Typed rejection returned by `get_directory_session_range` and
 * `revise_directory_session_view` instead of a plain error string, mirroring
 * Rust's `#[serde(tag = "kind")] SessionRejection` enum exactly. Every
 * variant except `pageOutOfRange` means "this baseline/session is stale —
 * silently re-navigate/re-fetch", not a user-facing failure.
 */
export type SessionRejection =
  | { kind: 'noActiveSession' }
  | { kind: 'staleSession' }
  | { kind: 'staleNavigation' }
  | { kind: 'staleWatch' }
  | { kind: 'staleView' }
  | { kind: 'pageOutOfRange' }

/** Rust-authored, revisioned watch update for an active directory session. */
export type SessionRowDelta =
  | { kind: 'inserted'; rowIndex: number; entry: DirectoryEntry }
  | { kind: 'removed'; rowIndex: number; path: string }
  | { kind: 'updated'; rowIndex: number; entry: DirectoryEntry }

export type SessionPatchEvent =
  | {
      mode: 'delta'
      paneId: string
      tabId: string
      path: string
      previousBaseline: SessionBaseline
      nextBaseline: SessionBaseline
      totalRows: number
      deltas: SessionRowDelta[]
    }
  | {
      mode: 'replaceView'
      paneId: string
      tabId: string
      path: string
      previousBaseline: SessionBaseline
      nextBaseline: SessionBaseline
      totalRows: number
    }
  | {
      mode: 'metadataOnly'
      paneId: string
      tabId: string
      path: string
      baseline: SessionBaseline
      updates: Array<{ path: string; entry: DirectoryEntry }>
    }
