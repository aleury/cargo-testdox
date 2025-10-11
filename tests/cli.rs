use assert_cmd::Command;
use predicates::prelude::*;
use regex::Regex;

#[test]
fn passing_include_ignored_flag_runs_ignored_tests() {
    Command::cargo_bin("cargo-testdox")
        .unwrap()
        .current_dir("testdata/testproj")
        .arg("testdox")
        .arg("--")
        .arg("--include-ignored")
        .assert()
        .success()
        .stdout(predicate::eq("✔ ignored test\n"));
}

#[test]
fn passing_include_ignored_flag_returns_unexpected_argument_error() {
    Command::cargo_bin("cargo-testdox")
        .unwrap()
        .current_dir("testdata/testproj")
        .arg("testdox")
        .arg("--include-ignored")
        .assert()
        .failure()
        .stderr(predicate::function(|output: &str| {
            let stderr = strip_ansi_codes(output);
            stderr.contains("error: unexpected argument '--include-ignored' found")
                && stderr.contains(
                    "tip: to pass '--include-ignored' as a value, use '-- --include-ignored'",
                )
                && !stderr.contains("Usage: cargo test")
        }));
}

fn strip_ansi_codes(s: &str) -> String {
    let re = Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    re.replace_all(s, "").to_string()
}
