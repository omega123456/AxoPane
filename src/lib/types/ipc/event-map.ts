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
  'size://state': SizeStateEvent[]
  'size://index-status': FolderSizeIndexStatus
  'icon://state': IconStateEvent[]
  'thumbnail://state': ThumbnailResultEvent[]
  'item-count://state': ItemCountEvent
  'volumes://changed': VolumesChangedEvent
  'queue://progress': QueueProgressEvent
  'queue://conflict': QueueConflictEvent
  'queue://removed': QueueRemovedEvent
  'watch://error': WatchErrorEvent
  'drag://position': DragPositionEvent
}
