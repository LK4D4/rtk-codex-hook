#[test]
fn release_workflow_requires_cargo_version_to_match_release_tag() {
    let workflow =
        std::fs::read_to_string(".github/workflows/release.yml").expect("read release workflow");

    assert!(
        workflow.contains("RELEASE_TAG: ${{ github.event_name == 'workflow_dispatch' && inputs.tag || github.ref_name }}"),
        "release workflow should derive a single release tag from push and manual runs"
    );
    assert!(
        workflow.contains("$cargoVersion") && workflow.contains("$tagVersion"),
        "release workflow should compare Cargo.toml package.version with the release tag"
    );
    assert!(
        workflow.contains("Cargo.toml version $cargoVersion does not match release tag $tag"),
        "release workflow should fail when Cargo.toml and the release tag diverge"
    );
    assert!(
        !workflow.contains("Set-Content -LiteralPath Cargo.toml")
            && !workflow.contains("cargo update -p rtk-codex-hook --precise"),
        "release workflow should not mutate Cargo.toml or Cargo.lock in the build workspace"
    );
}
