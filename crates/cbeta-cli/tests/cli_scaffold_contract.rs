//! Characterization tests for today's cbeta CLI scaffold contract.
//! Pins exit codes and `--json` Command dump (not Hits). Do not "upgrade" these.

fn cbeta() -> std::process::Command {
    std::process::Command::new(env!("CARGO_BIN_EXE_cbeta"))
}

#[test]
fn help_exits_0() {
    let out = cbeta().arg("--help").output().expect("spawn cbeta --help");
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn bare_search_exits_2() {
    // No --json / --explain: scaffold reports missing index and exits 2.
    let out = cbeta().arg("色即是空").output().expect("spawn bare search");
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn json_dumps_command_not_hits() {
    let out = cbeta()
        .args(["search", "--json", "空性"])
        .output()
        .expect("spawn search --json");
    assert_eq!(out.status.code(), Some(0));

    let cmd: cbeta_core::Command =
        serde_json::from_slice(&out.stdout).expect("stdout deserializes as Command");
    assert_eq!(cmd.action, cbeta_core::Action::Search);
    assert_eq!(cmd.q.as_deref(), Some("空性"));

    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout is JSON");
    let obj = value.as_object().expect("JSON object");
    assert!(
        !obj.contains_key("hits"),
        "scaffold --json dumps Command, not hits; keys={:?}",
        obj.keys().collect::<Vec<_>>()
    );
}
