//! Archive operation helpers live under the queue owner.
//!
//! ZIP execution is intentionally dispatched by `OpsService`; this module is
//! the stable home for archive-specific code as the remaining helpers are
//! separated from the queue shell.

use super::OpKind;

pub fn needs_cpu_admission(kind: OpKind) -> bool {
    matches!(kind, OpKind::Compress | OpKind::Extract)
}
