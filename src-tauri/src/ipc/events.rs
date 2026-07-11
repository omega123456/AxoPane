pub const DIR_PATCH: &str = "dir://patch";
pub const SIZE_STATE: &str = "size://state";
pub const ICON_STATE: &str = "icon://state";
pub const ITEM_COUNT: &str = "item-count://state";
pub const VOLUMES_CHANGED: &str = "volumes://changed";
pub const QUEUE_PROGRESS: &str = "queue://progress";
pub const QUEUE_CONFLICT: &str = "queue://conflict";
pub const QUEUE_REMOVED: &str = "queue://removed";
pub const WATCH_ERROR: &str = "watch://error";
/// v2 directory-session patch event (Phase 5): carries a `SessionPatch`
/// (`delta`/`replaceView`/`metadataOnly`) instead of the legacy tab-scoped
/// `dir://patch` shape. Distinct event name so the un-migrated fraction of
/// the app (still on `DIR_PATCH`) and the v2 `PaneEntryCollection` consumer
/// never need to disambiguate a shared channel.
pub const DIR_SESSION_PATCH: &str = "dir://session-patch";
