use super::OpStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusAction {
    Pause,
    Resume,
    Cancel,
    Skip,
    Retry,
    ResolveConflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RejectedTransition {
    pub from: OpStatus,
    pub action: StatusAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cancellation {
    BeforeStart,
    WhileRunning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    Cancelled,
    Failed,
    Completed,
}

impl OpStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    pub fn pause(self) -> Result<OpStatus, RejectedTransition> {
        self.transition(Transition {
            required: Self::Active,
            action: StatusAction::Pause,
            next: Self::Paused,
        })
    }

    pub fn resume(self) -> Result<OpStatus, RejectedTransition> {
        self.transition(Transition {
            required: Self::Paused,
            action: StatusAction::Resume,
            next: Self::Active,
        })
    }

    pub fn skip(self) -> Result<OpStatus, RejectedTransition> {
        self.transition(Transition {
            required: Self::Paused,
            action: StatusAction::Skip,
            next: Self::Active,
        })
    }

    pub fn retry(self) -> Result<OpStatus, RejectedTransition> {
        self.transition(Transition {
            required: Self::Failed,
            action: StatusAction::Retry,
            next: Self::Pending,
        })
    }

    pub fn resolve_conflict(self) -> Result<OpStatus, RejectedTransition> {
        self.transition(Transition {
            required: Self::Conflict,
            action: StatusAction::ResolveConflict,
            next: Self::Active,
        })
    }

    pub fn cancel(self) -> Result<Cancellation, RejectedTransition> {
        if self.is_terminal() {
            return Err(self.rejected(StatusAction::Cancel));
        }
        if self == Self::Pending {
            return Ok(Cancellation::BeforeStart);
        }
        Ok(Cancellation::WhileRunning)
    }

    pub fn finish(outcome: RunOutcome) -> OpStatus {
        match outcome {
            RunOutcome::Cancelled => Self::Cancelled,
            RunOutcome::Failed => Self::Failed,
            RunOutcome::Completed => Self::Completed,
        }
    }

    fn transition(self, transition: Transition) -> Result<OpStatus, RejectedTransition> {
        if self == transition.required {
            return Ok(transition.next);
        }
        Err(self.rejected(transition.action))
    }

    fn rejected(self, action: StatusAction) -> RejectedTransition {
        RejectedTransition { from: self, action }
    }
}

struct Transition {
    required: OpStatus,
    action: StatusAction,
    next: OpStatus,
}

impl Cancellation {
    pub fn next_status(self) -> OpStatus {
        OpStatus::Cancelled
    }
}
