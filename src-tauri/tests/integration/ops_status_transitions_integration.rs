use file_explorer_lib::ops::status::{Cancellation, RejectedTransition, RunOutcome, StatusAction};
use file_explorer_lib::ops::OpStatus;

const ALL_STATUSES: [OpStatus; 7] = [
    OpStatus::Pending,
    OpStatus::Active,
    OpStatus::Paused,
    OpStatus::Conflict,
    OpStatus::Completed,
    OpStatus::Failed,
    OpStatus::Cancelled,
];

fn rejected(from: OpStatus, action: StatusAction) -> RejectedTransition {
    RejectedTransition { from, action }
}

fn expected_transition(
    status: OpStatus,
    allowed_from: OpStatus,
    next: OpStatus,
    action: StatusAction,
) -> Result<OpStatus, RejectedTransition> {
    if status == allowed_from {
        return Ok(next);
    }
    Err(rejected(status, action))
}

fn assert_only_allowed_from(
    allowed_from: OpStatus,
    next: OpStatus,
    action: StatusAction,
    transition: fn(OpStatus) -> Result<OpStatus, RejectedTransition>,
) {
    for status in ALL_STATUSES {
        let expected = expected_transition(status, allowed_from, next, action);
        assert_eq!(transition(status), expected, "{status:?} {action:?}");
    }
}

#[test]
fn pause_resume_skip_retry_and_resolve_require_their_source_status() {
    assert_only_allowed_from(
        OpStatus::Active,
        OpStatus::Paused,
        StatusAction::Pause,
        OpStatus::pause,
    );
    assert_only_allowed_from(
        OpStatus::Paused,
        OpStatus::Active,
        StatusAction::Resume,
        OpStatus::resume,
    );
    assert_only_allowed_from(
        OpStatus::Paused,
        OpStatus::Active,
        StatusAction::Skip,
        OpStatus::skip,
    );
    assert_only_allowed_from(
        OpStatus::Failed,
        OpStatus::Pending,
        StatusAction::Retry,
        OpStatus::retry,
    );
    assert_only_allowed_from(
        OpStatus::Conflict,
        OpStatus::Active,
        StatusAction::ResolveConflict,
        OpStatus::resolve_conflict,
    );
}

#[test]
fn cancel_distinguishes_never_started_from_running_and_rejects_terminal() {
    assert_eq!(OpStatus::Pending.cancel(), Ok(Cancellation::BeforeStart));
    for running in [OpStatus::Active, OpStatus::Paused, OpStatus::Conflict] {
        assert_eq!(running.cancel(), Ok(Cancellation::WhileRunning));
    }
    for terminal in [OpStatus::Completed, OpStatus::Failed, OpStatus::Cancelled] {
        assert!(terminal.is_terminal());
        assert_eq!(
            terminal.cancel(),
            Err(rejected(terminal, StatusAction::Cancel))
        );
    }
    assert_eq!(Cancellation::BeforeStart.next_status(), OpStatus::Cancelled);
    assert_eq!(
        Cancellation::WhileRunning.next_status(),
        OpStatus::Cancelled
    );
}

#[test]
fn finish_maps_each_run_outcome_to_its_terminal_status() {
    assert_eq!(OpStatus::finish(RunOutcome::Cancelled), OpStatus::Cancelled);
    assert_eq!(OpStatus::finish(RunOutcome::Failed), OpStatus::Failed);
    assert_eq!(OpStatus::finish(RunOutcome::Completed), OpStatus::Completed);
    assert!(!OpStatus::Active.is_terminal());
}
