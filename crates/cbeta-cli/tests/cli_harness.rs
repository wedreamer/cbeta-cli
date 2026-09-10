//! Helper-unit contracts for Wave 1 harness (`tests/common/mod.rs`).
//!
//! These pin fail-closed CBETA_FULL gating, synthetic FETCHED fixtures, and
//! isolated HOME — not product CLI surface.

mod common;

use common::{
    cbeta_full_enabled, lock_pins_for, mini_corpus, plant_mini_under_tag, skip_unless_cbeta_full,
    temp_dir, write_complete_fetched_yaml,
};
use std::fs;
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

#[test]
fn lock_pins_for_2026r2_parses_lock_file() {
    // Given / When: parse lock via helper (helper must not hard-code SHAs)
    let pins = lock_pins_for("2026R2");
    // Then: pins match releases.lock.yaml 2026R2
    assert_eq!(pins.xml_p5, "dbdea41071e1e260ad84b72faefd4587333cf76d");
    assert_eq!(pins.metadata, "af914b8d3ce6b7c929cbef5cef061e00bddda929");
    assert_eq!(pins.gaiji, "30fb27a9fb7b8619d0f1c7a78c90c0a60108a067");
}

#[test]
fn write_complete_fetched_yaml_never_latest_and_matches_pins() {
    let home = temp_dir("fetched-home");
    let tag = "2026R2";
    let tag_dir = home.join(".cbeta").join("corpus").join(tag);
    fs::create_dir_all(&tag_dir).unwrap_or_else(|e| panic!("mkdir tag_dir: {e}"));
    let pins = lock_pins_for(tag);

    write_complete_fetched_yaml(&tag_dir, tag, &pins);

    let raw = fs::read_to_string(tag_dir.join("FETCHED.yaml"))
        .unwrap_or_else(|e| panic!("read FETCHED: {e}"));
    assert!(
        raw.contains("cbeta-cli.fetched/v1"),
        "schema missing: {raw}"
    );
    assert!(
        raw.contains("2026R2"),
        "cbeta_release must be concrete tag: {raw}"
    );
    assert!(
        !raw.contains("cbeta_release: latest") && !raw.contains("cbeta_release: \"latest\""),
        "cbeta_release must never be latest: {raw}"
    );
    assert!(raw.contains(&pins.xml_p5), "xml-p5 commit missing: {raw}");
    assert!(
        raw.contains(&pins.metadata),
        "metadata commit missing: {raw}"
    );
    assert!(raw.contains(&pins.gaiji), "gaiji commit missing: {raw}");

    let mini_fetched = mini_corpus().join("FETCHED.yaml");
    let mini_raw =
        fs::read_to_string(&mini_fetched).unwrap_or_else(|e| panic!("read mini FETCHED: {e}"));
    assert!(
        mini_raw.contains("note:"),
        "mini FETCHED must remain incomplete tag+note: {mini_raw}"
    );
    assert!(
        !mini_raw.contains("cbeta-cli.fetched/v1"),
        "must not overwrite in-repo mini FETCHED"
    );
}

#[test]
fn plant_mini_under_tag_copies_xml_p5_layout() {
    let home = temp_dir("plant-home");
    let tag = "2026R2";
    let tag_dir = plant_mini_under_tag(&home, tag);
    assert_eq!(tag_dir, home.join(".cbeta").join("corpus").join(tag));
    assert!(
        tag_dir.join("xml-p5").is_dir(),
        "expected $home/.cbeta/corpus/2026R2/xml-p5 under {}",
        tag_dir.display()
    );
    assert!(
        !home.join(".cbeta").join("corpus").join("CURRENT").exists(),
        "plant must not write CURRENT"
    );
}
