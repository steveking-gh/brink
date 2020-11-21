#[cfg(test)]
mod tests {
use assert_cmd::{Command};
use predicates::prelude::*;
use std::path::Path;
use std::fs;

#[test]
fn help_only() {
    let _cmd = Command::cargo_bin("roust")
                .unwrap()
                .arg("--help")
                .unwrap();
}

#[test]
#[should_panic]
fn missing_input() {
    let _cmd = Command::cargo_bin("roust")
                .unwrap()
                .arg("does_not_exist.roust")
                .unwrap();
}

#[test]
#[should_panic]
fn no_cli_input() {
    let _cmd = Command::cargo_bin("roust")
                .unwrap()
                .unwrap();
}

#[test]
fn empty_1() {
    let _cmd = Command::cargo_bin("roust")
                .unwrap()
                .arg("tests/empty_1.roust")
                .assert()
                .failure();
}

#[test]
fn empty_2() {
    let _cmd = Command::cargo_bin("roust")
                .unwrap()
                .arg("tests/empty_2.roust")
                .assert()
                .failure();
}

#[test]
fn line_comment_1() {
    let _cmd = Command::cargo_bin("roust")
                .unwrap()
                .arg("tests/line_comment_1.roust")
                .assert()
                .success();
}

#[test]
fn line_comment_2() {
    let _cmd = Command::cargo_bin("roust")
                .unwrap()
                .arg("tests/line_comment_2.roust")
                .assert()
                .success();
}

#[test]
fn multi_comment_1() {
    let _cmd = Command::cargo_bin("roust")
                .unwrap()
                .arg("tests/multi_comment_1.roust")
                .assert()
                .success();
}

#[test]
fn multi_comment_2() {
    let _cmd = Command::cargo_bin("roust")
                .unwrap()
                .arg("tests/multi_comment_2.roust")
                .assert()
                .success();
}

#[test]
fn multi_comment_3() {
    let _cmd = Command::cargo_bin("roust")
                .unwrap()
                .arg("tests/multi_comment_3.roust")
                .assert()
                .success();
}

#[test]
fn simple_section_1() {
    let _cmd = Command::cargo_bin("roust")
                .unwrap()
                .arg("tests/simple_section_1.roust")
                .assert()
                .success();

    let p_is_empty = predicate::str::is_empty().from_utf8().from_file_path();
    assert!(p_is_empty.eval(Path::new("empty.bin")));

    // File content good, clean up.
    fs::remove_file("empty.bin").unwrap();

}

#[test]
fn simple_section_2() {
    let _cmd = Command::cargo_bin("roust")
                .unwrap()
                .arg("tests/simple_section_2.roust")
                .assert()
                .success();

    // Verify output file is correct
    let p_eq = predicates::path::eq_file(Path::new("test.bin")).utf8().unwrap();
    assert!(p_eq.eval("Wow!"));

    // File content good, clean up.
    fs::remove_file("test.bin").unwrap();
}

#[test]
fn section_rename_err_1() {
    let _cmd = Command::cargo_bin("roust")
    .unwrap()
    .arg("tests/section_rename_err_1.roust")
    .assert()
    .failure();
}

#[test]
fn no_output_warn_1() {
    let _cmd = Command::cargo_bin("roust")
    .unwrap()
    .arg("tests/no_output_warn_1.roust")
    .assert()
    .success()
    .stderr(predicates::str::contains("WARN_10"));
}

} // mod tests

