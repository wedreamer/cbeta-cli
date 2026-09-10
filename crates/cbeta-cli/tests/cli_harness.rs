//! Helper-unit contracts for Wave 1 harness (`tests/common/mod.rs`).
//!
//! These pin fail-closed CBETA_FULL gating, synthetic FETCHED fixtures, and
//! isolated HOME — not product CLI surface.

mod common;

use common::{cbeta_full_enabled, skip_unless_cbeta_full};
use std::sync::{Mutex, OnceLock};

/// Serialize env mutation across harness tests (process-global `std::env`).
fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

#[test]
fn cbeta_full_enabled_false_when_unset() {
    let _g = env_lock();
    // Given: CBETA_FULL is not set for this process
    std::env::remove_var("CBETA_FULL");
    // When / Then: fail-closed — scholar lane off
    assert!(!cbeta_full_enabled());
    assert!(
        skip_unless_cbeta_full(),
        "unset CBETA_FULL must tell caller to return early"
    );
}

#[test]
fn cbeta_full_enabled_false_when_empty_or_zero() {
    let _g = env_lock();
    std::env::set_var("CBETA_FULL", "");
    assert!(!cbeta_full_enabled());
    assert!(skip_unless_cbeta_full());

    std::env::set_var("CBETA_FULL", "0");
    assert!(!cbeta_full_enabled());
    assert!(skip_unless_cbeta_full());

    std::env::remove_var("CBETA_FULL");
}

#[test]
fn cbeta_full_enabled_true_only_when_one() {
    let _g = env_lock();
    std::env::set_var("CBETA_FULL", "1");
    assert!(cbeta_full_enabled());
    assert!(!skip_unless_cbeta_full(), "CBETA_FULL=1 must not skip");
    std::env::remove_var("CBETA_FULL");
}
