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

#[derive(Clone, Copy)]
struct AllowedTransition {
    allowed_from: OpStatus,
    next: OpStatus,
    action: StatusAction,
}

fn expected_transition(
    status: OpStatus,
    rule: AllowedTransition,
) -> Result<OpStatus, RejectedTransition> {
    if status == rule.allowed_from {
        return Ok(rule.next);
    }
    Err(rejected(status, rule.action))
}

fn assert_only_allowed_from(
    rule: AllowedTransition,
    transition: fn(OpStatus) -> Result<OpStatus, RejectedTransition>,
) {
    for status in ALL_STATUSES {
        let expected = expected_transition(status, rule);
        let action = rule.action;
        assert_eq!(transition(status), expected, "{status:?} {action:?}");
    }
}

#[test]
fn pause_resume_skip_retry_and_resolve_require_their_source_status() {
    assert_only_allowed_from(
        AllowedTransition {
            allowed_from: OpStatus::Active,
            next: OpStatus::Paused,
            action: StatusAction::Pause,
        },
        OpStatus::pause,
    );
    assert_only_allowed_from(
        AllowedTransition {
            allowed_from: OpStatus::Paused,
            next: OpStatus::Active,
            action: StatusAction::Resume,
        },
        OpStatus::resume,
    );
    assert_only_allowed_from(
        AllowedTransition {
            allowed_from: OpStatus::Paused,
            next: OpStatus::Active,
            action: StatusAction::Skip,
        },
        OpStatus::skip,
    );
    assert_only_allowed_from(
        AllowedTransition {
            allowed_from: OpStatus::Failed,
            next: OpStatus::Pending,
            action: StatusAction::Retry,
        },
        OpStatus::retry,
    );
    assert_only_allowed_from(
        AllowedTransition {
            allowed_from: OpStatus::Conflict,
            next: OpStatus::Active,
            action: StatusAction::ResolveConflict,
        },
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
