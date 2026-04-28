//! Volume step regression tests
//!
//! Bug: Adapters hardcode volume_step to 1.0, ignoring backend-specific values.
//!
//!
//! Test strategy:
//! 1. Source-scanning lint tests (catch obvious regressions)
//! 2. Unit tests calling pub(crate) conversion functions (verify behavior)

use std::fs;

// =============================================================================
// LINT TESTS: Source scanning to catch hardcoded step values
// These are a first line of defense, not a replacement for unit tests.
// =============================================================================

/// Roon adapter must not hardcode step: 1.0 - API provides the value.
#[test]
fn lint_roon_no_hardcoded_step() {
    let src =
        fs::read_to_string("src/adapters/roon.rs").expect("Failed to read src/adapters/roon.rs");

    // Bug: roon_zone_to_bus_zone hardcodes "step: 1.0"
    // Fix: use v.step.unwrap_or(1.0)
    let has_hardcoded = src.contains("step: 1.0,");

    assert!(
        !has_hardcoded,
        "REGRESSION: Roon adapter hardcodes 'step: 1.0'.\n\
         Fix: Use v.step.unwrap_or(1.0) in roon_zone_to_bus_zone()"
    );
}

// =============================================================================
// CONTROL HANDLER TESTS: Ensure vol_up/vol_down respect fractional steps
// =============================================================================

/// Roon adapter change_volume must accept f32, not i32
/// Bug: i32 truncates fractional steps like 0.5 to 0
#[test]
fn lint_roon_change_volume_uses_f32() {
    let src =
        fs::read_to_string("src/adapters/roon.rs").expect("Failed to read src/adapters/roon.rs");

    // The function signature must use f32 for fractional step support
    let uses_f32 = src.contains("fn change_volume(&self, zone_id: &str, value: f32");

    assert!(
        uses_f32,
        "REGRESSION: Roon change_volume must use f32, not i32.\n\
         i32 truncates fractional steps (0.5 → 0).\n\
         Fix: Change signature to 'value: f32'"
    );
}
