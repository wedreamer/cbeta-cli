//! Product contract: `cbeta build --scope ci-minimal` against the in-repo mini fixture.

mod common;

use common::{cbeta_env, mini_corpus, temp_dir};

#[test]
fn build_ci_minimal_exits_0_and_writes_index() {
    // Given: mini corpus + empty index root via env
    let corpus = mini_corpus();
    let index = temp_dir("build-idx");

    // When: build --scope ci-minimal
    let out = cbeta_env(&corpus, &index)
        .args(["build", "--scope", "ci-minimal"])
        .output()
        .expect("spawn build");

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(0),
        "build should exit 0; stdout={stdout} stderr={stderr}"
    );

    // Then: artifact dir exists under CBETA_INDEX
    let artifact = index.join("2026R2-c1f1x7a0");
    assert!(
        artifact.is_dir(),
        "expected index artifact at {}; entries={:?}",
        artifact.display(),
        std::fs::read_dir(&index)
            .map(|rd| rd
                .filter_map(|e| e.ok().map(|e| e.file_name()))
                .collect::<Vec<_>>())
            .unwrap_or_default()
    );
    assert!(
        artifact.join("meta.json").is_file() || artifact.join("MANIFEST.json").is_file(),
        "expected meta/MANIFEST sidecar in artifact"
    );
}
