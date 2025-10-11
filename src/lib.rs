#![doc = include_str!("../README.md")]
use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use std::{
    collections::{BTreeMap, HashMap},
    fmt::Display,
    process::{Command, ExitStatus},
    str::FromStr,
};

#[derive(Debug)]
pub struct Output {
    pub results: Vec<TestResult>,
    pub error: Option<CargoError>,
    pub status: ExitStatus,
}

impl Output {
    #[must_use]
    pub fn failed(&self) -> bool {
        !self.status.success()
            || self
                .results
                .iter()
                .any(|result| result.status == Status::Fail)
    }
}

#[must_use]
/// Runs `cargo test` with any supplied extra arguments, and returns the
/// resulting standard output.
///
/// # Panics
///
/// If executing the `cargo test` command fails.
pub fn get_cargo_test_output(extra_args: Vec<String>) -> Output {
    let mut cargo = Command::new("cargo");
    cargo.arg("test");
    cargo.args(extra_args);
    let cargo_output = cargo
        .env("CARGO_TERM_COLOR", "always")
        .output()
        .context(format!("{cargo:?}"))
        .expect("executing command should succeed");

    let stdout = String::from_utf8_lossy(&cargo_output.stdout).to_string();
    let results = parse_test_results(&stdout).expect("parsing test results should succeed");
    let mut output = Output {
        results,
        error: None,
        status: cargo_output.status,
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&cargo_output.stderr);
        output.error = Some(parse_error(&stderr));
    }
    output
}

/// Parses the standard output of `cargo test` into a vec of `TestResult`.
///
/// # Errors
///
/// Returns an error if the test output cannot be parsed.
pub fn parse_test_results(test_output: &str) -> Result<Vec<TestResult>> {
    let mut results = BTreeMap::new();
    let failures = parse_test_failures(test_output).context("parsing test failures")?;

    for line in test_output.lines() {
        if let Some(mut result) = parse_line(line) {
            if result.status == Status::Fail {
                result.failure = failures.get(&result.test).cloned();
            }
            results.insert(result.test.clone(), result);
        }
    }

    Ok(results.into_values().collect())
}

#[derive(Debug, Clone, PartialEq)]
pub struct FailureInfo {
    pub message: String,
    pub location: String,
}

impl std::fmt::Display for FailureInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "At {}\n{}", self.location, self.message)
    }
}

#[must_use]
pub fn parse_test_failures(output: &str) -> Option<HashMap<String, FailureInfo>> {
    let mut failures = HashMap::new();
    let lines: Vec<&str> = output.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        if line.starts_with("thread") && line.contains("panicked at") {
            let (test, location) = line
                .strip_prefix("thread '")?
                .split_once("' panicked at ")
                .map(|(test, loc)| (test.into(), loc.into()))?;
            let message = lines[i + 1..]
                .iter()
                .take_while(|l| !l.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join("\n");
            failures.insert(test, FailureInfo { message, location });
        }
    }

    Some(failures)
}

/// Parses a line from the standard output of `cargo test`.
///
/// If the line represents the result of a test, returns `Some(TestResult)`,
/// otherwise returns `None`.
pub fn parse_line(line: impl AsRef<str>) -> Option<TestResult> {
    let line = line.as_ref().strip_prefix("test ")?;
    if line.starts_with("result") || line.contains("(line ") {
        return None;
    }

    let (test, status) = line.split_once(" ... ")?;
    let (module, name) = match test.rsplit_once("::") {
        Some((module, name)) => (prettify_module(module), name),
        None => (None, test),
    };
    Some(TestResult {
        module,
        test: test.into(),
        name: prettify(name),
        status: status.parse().ok()?,
        failure: None,
    })
}

#[must_use]
/// Formats the name of a test function as a sentence.
///
/// Underscores are replaced with spaces. To retain the underscores in a function name, put `_fn_` after it. For example:
///
/// ```text
/// parse_line_fn_parses_a_line
/// ```
///
/// becomes:
///
/// ```text
/// parse_line parses a line
/// ```
pub fn prettify(input: impl AsRef<str>) -> String {
    if let Some((fn_name, sentence)) = input.as_ref().split_once("_fn_") {
        format!("{} {}", fn_name, humanize(sentence))
    } else {
        humanize(input)
    }
}

fn humanize(input: impl AsRef<str>) -> String {
    input
        .as_ref()
        .replace('_', " ")
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ")
}

fn prettify_module(module: &str) -> Option<String> {
    let mut parts = module.split("::").collect::<Vec<_>>();
    parts.pop_if(|&mut s| s == "tests" || s == "test");
    if parts.is_empty() {
        return None;
    }
    Some(parts.join("::"))
}

#[derive(Debug, PartialEq)]
/// The (prettified) name and pass/fail status of a given test.
pub struct TestResult {
    pub module: Option<String>,
    pub test: String,
    pub name: String,
    pub status: Status,
    pub failure: Option<FailureInfo>,
}

impl Display for TestResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.module {
            Some(module) => write!(
                f,
                "{} {} – {}{}",
                self.status,
                module.bright_blue(),
                self.name,
                self.failure
                    .as_ref()
                    .map_or(String::new(), |failure| format!("\n{failure}"))
            ),
            None => write!(f, "{} {}", self.status, self.name),
        }
    }
}

#[derive(Debug, PartialEq)]
/// The status of a given test, as reported by `cargo test`.
pub enum Status {
    Pass,
    Fail,
    Ignored,
}

impl FromStr for Status {
    type Err = anyhow::Error;

    fn from_str(status: &str) -> Result<Self, Self::Err> {
        match status {
            "ok" => Ok(Status::Pass),
            "FAILED" => Ok(Status::Fail),
            "ignored" => Ok(Status::Ignored),
            _ => Err(anyhow!("unhandled test status {status:?}")),
        }
    }
}

impl Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = match self {
            Status::Pass => "✔".bright_green(),
            Status::Fail => "x".bright_red(),
            Status::Ignored => "?".bright_yellow(),
        };
        write!(f, "{status}")
    }
}

#[derive(Debug, PartialEq)]
pub struct CargoError {
    message: String,
    tip: Option<String>,
}

impl std::fmt::Display for CargoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}\n", self.message)?;
        if let Some(tip) = &self.tip {
            writeln!(f, "    {tip}")?;
        }
        Ok(())
    }
}

#[must_use]
pub fn parse_error(stderr: &str) -> CargoError {
    let lines: Vec<_> = stderr.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        if line.contains("error") {
            let message = line.trim().to_string();
            let mut error = CargoError { message, tip: None };

            let mut other_lines = Vec::new();
            for next_line in &lines[i + 1..lines.len()] {
                if next_line.contains("error:") || next_line.contains("warning:") {
                    break;
                }
                if next_line.contains("Usage:") {
                    break;
                }
                other_lines.push(next_line);
            }

            for line in &other_lines {
                if line.trim().contains("tip:") {
                    error.tip = Some(line.trim().to_string());
                }
            }

            return error;
        }
    }

    CargoError {
        message: stderr.trim().to_string(),
        tip: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prettify_returns_expected_results() {
        struct Case {
            input: &'static str,
            want: String,
        }
        let cases = Vec::from([
            Case {
                input: "anagrams_must_use_all_letters_exactly_once",
                want: "anagrams must use all letters exactly once".into(),
            },
            Case {
                input: "no_matches",
                want: "no matches".into(),
            },
            Case {
                input: "single",
                want: "single".into(),
            },
            Case {
                input: "parse_line_fn_does_stuff",
                want: "parse_line does stuff".into(),
            },
            Case {
                input: "prettify__handles_multiple_underscores",
                want: "prettify handles multiple underscores".into(),
            },
            Case {
                input: "prettify_fn__handles_multiple_underscores",
                want: "prettify handles multiple underscores".into(),
            },
        ]);
        for case in cases {
            assert_eq!(case.want, prettify(case.input));
        }
    }

    #[test]
    fn parse_line_fn_returns_expected_result() {
        struct Case {
            line: &'static str,
            want: Option<TestResult>,
        }
        let cases = Vec::from([
            Case {
                line: "    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.20s",
                want: None,
            },
            Case {
                line: "test foo ... ok",
                want: Some(TestResult {
                    test: "foo".into(),
                    module: None,
                    name: "foo".into(),
                    status: Status::Pass,
                    failure: None,
                }),
            },
            Case {
                line: "test foo::tests::does_foo_stuff ... ok",
                want: Some(TestResult {
                    test: "foo::tests::does_foo_stuff".into(),
                    module: Some("foo".into()),
                    name: "does foo stuff".into(),
                    status: Status::Pass,
                    failure: None,
                }),
            },
            Case {
                line: "test tests::urls_correctly_extracts_valid_urls ... FAILED",
                want: Some(TestResult {
                    test: "tests::urls_correctly_extracts_valid_urls".into(),
                    module: None,
                    name: "urls correctly extracts valid urls".into(),
                    status: Status::Fail,
                    failure: None,
                }),
            },
            Case {
                line: "test files::test::files_can_be_sorted_in_descending_order ... ignored",
                want: Some(TestResult {
                    test: "files::test::files_can_be_sorted_in_descending_order".into(),
                    module: Some("files".into()),
                    name: "files can be sorted in descending order".into(),
                    status: Status::Ignored,
                    failure: None,
                }),
            },
            Case {
                line: "test files::test::foo::tests::files_can_be_sorted_in_descending_order ... ignored",
                want: Some(TestResult {
                    test: "files::test::foo::tests::files_can_be_sorted_in_descending_order".into(),
                    module: Some("files::test::foo".into()),
                    name: "files can be sorted in descending order".into(),
                    status: Status::Ignored,
                    failure: None,
                }),
            },
            Case {
                line: "test files::test_foo::files_can_be_sorted_in_descending_order ... ignored",
                want: Some(TestResult {
                    test: "files::test_foo::files_can_be_sorted_in_descending_order".into(),
                    module: Some("files::test_foo".into()),
                    name: "files can be sorted in descending order".into(),
                    status: Status::Ignored,
                    failure: None,
                }),
            },
            Case {
                line: "test src/lib.rs - find_top_n_largest_files (line 17) ... ok",
                want: None,
            },
            Case {
                line: "test output_format::_concise_expects ... ok",
                want: Some(TestResult {
                    test: "output_format::_concise_expects".into(),
                    module: Some("output_format".into()),
                    name: "concise expects".into(),
                    status: Status::Pass,
                    failure: None,
                }),
            },
        ]);
        for case in cases {
            assert_eq!(case.want, parse_line(case.line));
        }
    }

    #[test]
    fn parse_error_fn_returns_unexpected_argument_error() {
        let error = r"error: unexpected argument '--ignore-included' found

    tip: to pass '--ignore-included' as a value, use '-- --ignore-included'

Usage: cargo test [OPTIONS] [TESTNAME] [-- [ARGS]...]

For more information, try '--help'.";

        let want = CargoError {
            message: "error: unexpected argument '--ignore-included' found".to_string(),
            tip: Some(
                "tip: to pass '--ignore-included' as a value, use '-- --ignore-included'".into(),
            ),
        };
        let got = parse_error(error);

        assert_eq!(want, got);
    }

    #[test]
    fn parse_test_results_fn_returns_test_result_with_failure_details() {
        let output = include_str!("../testdata/failure_output.txt");

        let want = vec![TestResult {
            test: "machine::tests::machine_step_executes_one_instruction".into(),
            module: Some("machine".into()),
            name: "machine step executes one instruction".into(),
            status: Status::Fail,
            failure: Some(FailureInfo {
                message: r"assertion `left == right` failed: Program counter should be incremented after executing a `halt` instruction
    left: 2
    right: 1
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace".into(),
                location: "crates/rmachine-core/src/machine.rs:133:9:".into(),
            }),
        }];

        let got = parse_test_results(output).unwrap();

        assert_eq!(want, got);
    }
}
