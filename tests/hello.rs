//! Smoke tests for the crate's public surface.
use tessera::hello;

#[test]
fn hello_greets() {
    assert_eq!(hello(), "hello from tessera");
}

#[test]
fn version_matches_manifest() {
    assert_eq!(tessera::VERSION, env!("CARGO_PKG_VERSION"));
}
