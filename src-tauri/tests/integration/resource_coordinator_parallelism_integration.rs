use file_explorer_lib::resource_coordinator::parallelism::{
    max_throughput_slots, scan_workers, scan_workers_for_tests,
    throughput_slots_for_parallelism_for_tests, walker_helper_budget,
    walker_helper_budget_for_tests, walker_job_threads, walker_job_threads_for_tests,
};

const UNDETECTED: Option<usize> = None;

#[test]
fn walker_job_threads_use_half_of_the_cpus_between_two_and_eight() {
    assert_eq!(walker_job_threads_for_tests(UNDETECTED), 2);
    assert_eq!(walker_job_threads_for_tests(Some(1)), 2);
    assert_eq!(walker_job_threads_for_tests(Some(4)), 2);
    assert_eq!(walker_job_threads_for_tests(Some(5)), 3);
    assert_eq!(walker_job_threads_for_tests(Some(8)), 4);
    assert_eq!(walker_job_threads_for_tests(Some(18)), 8);
    assert_eq!(walker_job_threads_for_tests(Some(64)), 8);
}

#[test]
fn walker_helper_budget_matches_the_cpus_between_two_and_sixteen() {
    assert_eq!(walker_helper_budget_for_tests(UNDETECTED), 2);
    assert_eq!(walker_helper_budget_for_tests(Some(1)), 2);
    assert_eq!(walker_helper_budget_for_tests(Some(4)), 4);
    assert_eq!(walker_helper_budget_for_tests(Some(8)), 8);
    assert_eq!(walker_helper_budget_for_tests(Some(18)), 16);
    assert_eq!(walker_helper_budget_for_tests(Some(64)), 16);
}

#[test]
fn scan_workers_use_half_of_the_cpus_between_one_and_eight() {
    assert_eq!(scan_workers_for_tests(UNDETECTED), 1);
    assert_eq!(scan_workers_for_tests(Some(1)), 1);
    assert_eq!(scan_workers_for_tests(Some(4)), 2);
    assert_eq!(scan_workers_for_tests(Some(8)), 4);
    assert_eq!(scan_workers_for_tests(Some(18)), 8);
    assert_eq!(scan_workers_for_tests(Some(64)), 8);
}

#[test]
fn an_eight_cpu_machine_keeps_the_previous_flat_walker_limits() {
    assert_eq!(walker_job_threads_for_tests(Some(8)), 4);
    assert_eq!(walker_helper_budget_for_tests(Some(8)), 8);
}

#[test]
fn detected_policies_stay_inside_their_bounds_and_never_change() {
    assert!((2..=8).contains(&max_throughput_slots()));
    assert!((2..=8).contains(&walker_job_threads()));
    assert!((2..=16).contains(&walker_helper_budget()));
    assert!((1..=8).contains(&scan_workers()));
    assert_eq!(walker_job_threads(), walker_job_threads());
    assert!(walker_job_threads() <= walker_helper_budget());
    assert_eq!(throughput_slots_for_parallelism_for_tests(UNDETECTED), 2);
}
