//! Test-side assertion helpers.

use crate::result::RunResult;

pub fn hook_fired(_r: &RunResult, _target_path: &str) {
    panic!("not yet implemented");
}

pub fn block_recorded(_r: &RunResult, _rule_id: &str) {
    panic!("not yet implemented");
}

pub fn pattern_absent(_r: &RunResult, _pattern: &str) {
    panic!("not yet implemented");
}
