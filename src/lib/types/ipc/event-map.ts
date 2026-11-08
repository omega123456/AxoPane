import type { DragPositionEvent } from './entries'
import type {
  QueueConflictEvent,
  QueueProgressEvent,
  QueueRemovedEvent,
  WatchErrorEvent,
} from './queue'
import type { SessionPatchEvent } from './session'
import type {
  DirPatchEvent,
  FolderSizeIndexStatus,
  IconStateEvent,
  SizeStateEvent,
  ThumbnailResultEvent,
} from './sizes'
import type { ItemCountEvent, VolumesChangedEvent } from './watch'

export type IpcEventMap = {
  'dir://patch': DirPatchEvent
  'dir://session-patch': SessionPatchEvent
  /** Batched: the backend flushes folder-size state changes in arrays to avoid IPC floods. */
  'size://state': SizeStateEvent[]
  'size://index-status': FolderSizeIndexStatus
  /** Batched: the backend flushes resolved icons in chunks. */
  'icon://state': IconStateEvent[]
  /** Batched contextual native thumbnail outcomes. */
  'thumbnail://state': ThumbnailResultEvent[]
  'item-count://state': ItemCountEvent
  'volumes://changed': VolumesChangedEvent
  'queue://progress': QueueProgressEvent
  'queue://conflict': QueueConflictEvent
  'queue://removed': QueueRemovedEvent
  'watch://error': WatchErrorEvent
  /** Cursor position during a native drag, in CSS pixels relative to the webview. */
  'drag://position': DragPositionEvent
}
