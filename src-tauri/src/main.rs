#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(not(feature = "test-utils"))]
    if axopane_lib::native_menu::helper_entry::try_run_from_args() {
        return;
    }
    axopane_lib::run()
}
