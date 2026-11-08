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
        self.transition(Self::Active, StatusAction::Pause, Self::Paused)
    }

    pub fn resume(self) -> Result<OpStatus, RejectedTransition> {
        self.transition(Self::Paused, StatusAction::Resume, Self::Active)
    }

    pub fn skip(self) -> Result<OpStatus, RejectedTransition> {
        self.transition(Self::Paused, StatusAction::Skip, Self::Active)
    }

    pub fn retry(self) -> Result<OpStatus, RejectedTransition> {
        self.transition(Self::Failed, StatusAction::Retry, Self::Pending)
    }

    pub fn resolve_conflict(self) -> Result<OpStatus, RejectedTransition> {
        self.transition(Self::Conflict, StatusAction::ResolveConflict, Self::Active)
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

    fn transition(
        self,
        required: OpStatus,
        action: StatusAction,
        next: OpStatus,
    ) -> Result<OpStatus, RejectedTransition> {
        if self == required {
            return Ok(next);
        }
        Err(self.rejected(action))
    }

    fn rejected(self, action: StatusAction) -> RejectedTransition {
        RejectedTransition { from: self, action }
    }
}

impl Cancellation {
    pub fn next_status(self) -> OpStatus {
        OpStatus::Cancelled
    }
}
