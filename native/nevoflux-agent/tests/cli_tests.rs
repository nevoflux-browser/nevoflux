//! CLI integration tests.
//!
//! These tests verify the command-line interface behavior of the nevoflux binary.

#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn test_help_output() {
    Command::cargo_bin("nevoflux-agent")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("NevoFlux"));
}

#[test]
fn test_version_output() {
    Command::cargo_bin("nevoflux-agent")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::is_match(r"nevoflux \d+\.\d+\.\d+").unwrap());
}

#[test]
fn test_status_not_running() {
    let temp = tempfile::TempDir::new().unwrap();

    Command::cargo_bin("nevoflux-agent")
        .unwrap()
        .arg("--status")
        .env("NEVOFLUX_DATA_DIR", temp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("not running"));
}

#[test]
fn test_stop_when_not_running() {
    let temp = tempfile::TempDir::new().unwrap();

    Command::cargo_bin("nevoflux-agent")
        .unwrap()
        .arg("--stop")
        .env("NEVOFLUX_DATA_DIR", temp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("not running"));
}

#[test]
fn test_status_with_stale_files() {
    let temp = tempfile::TempDir::new().unwrap();

    // Create stale daemon files
    std::fs::write(temp.path().join("daemon.port"), "19500").unwrap();
    std::fs::write(temp.path().join("daemon.pid"), "99999").unwrap();

    Command::cargo_bin("nevoflux-agent")
        .unwrap()
        .arg("--status")
        .env("NEVOFLUX_DATA_DIR", temp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("stale files"));
}

#[test]
fn test_stop_cleans_stale_files() {
    let temp = tempfile::TempDir::new().unwrap();

    // Create stale daemon files
    let port_file = temp.path().join("daemon.port");
    let pid_file = temp.path().join("daemon.pid");
    std::fs::write(&port_file, "19500").unwrap();
    std::fs::write(&pid_file, "99999").unwrap();

    Command::cargo_bin("nevoflux-agent")
        .unwrap()
        .arg("--stop")
        .env("NEVOFLUX_DATA_DIR", temp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Cleanup complete"));

    // Verify files are cleaned
    assert!(!port_file.exists());
    assert!(!pid_file.exists());
}
