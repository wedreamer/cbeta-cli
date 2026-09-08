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
        artifact.join("cbeta-meta.json").is_file() || artifact.join("MANIFEST.json").is_file(),
        "expected meta/MANIFEST sidecar in artifact"
    );
}

#[test]
fn build_resolves_fetch_src_xml_p5_layout() {
    // Given: fetch.sh cache layout (xml under src/xml-p5, not xml-p5/)
    let mini = mini_corpus();
    let corpus = temp_dir("src-xml-corpus");
    copy_tree(&mini.join("scopes"), &corpus.join("scopes"));
    copy_tree(&mini.join("xml-p5"), &corpus.join("src/xml-p5"));
    if mini.join("FETCHED.yaml").is_file() {
        std::fs::copy(mini.join("FETCHED.yaml"), corpus.join("FETCHED.yaml")).expect("fetched");
    }
    let index = temp_dir("src-xml-idx");

    // When: build --scope ci-minimal
    let out = cbeta_env(&corpus, &index)
        .args(["build", "--scope", "ci-minimal"])
        .output()
        .expect("spawn build");
    assert_eq!(
        out.status.code(),
        Some(0),
        "src/xml-p5 layout should build; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(index.join("2026R2-c1f1x7a0").is_dir());
}

#[allow(clippy::expect_used)]
fn copy_tree(src: &std::path::Path, dst: &std::path::Path) {
    std::fs::create_dir_all(dst).expect("mkdir");
    for ent in std::fs::read_dir(src).expect("read_dir") {
        let ent = ent.expect("dirent");
        let to = dst.join(ent.file_name());
        if ent.file_type().expect("ft").is_dir() {
            copy_tree(&ent.path(), &to);
        } else {
            std::fs::copy(ent.path(), to).expect("copy");
        }
    }
}
