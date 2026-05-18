#[test]
fn release_workflow_syncs_cargo_version_from_release_tag() {
    let workflow =
        std::fs::read_to_string(".github/workflows/release.yml").expect("read release workflow");

    assert!(
        workflow.contains("RELEASE_TAG: ${{ github.event_name == 'workflow_dispatch' && inputs.tag || github.ref_name }}"),
        "release workflow should derive a single release tag from push and manual runs"
    );
    assert!(
        workflow.contains("Cargo.toml") && workflow.contains("$version"),
        "release workflow should update Cargo.toml package.version before building"
    );
    assert!(
        workflow.contains("cargo update -p rtk-codex-hook --precise $version"),
        "release workflow should keep Cargo.lock aligned with the release version"
    );
}
