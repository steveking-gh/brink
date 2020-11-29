#[cfg(test)]
mod tests {
use assert_cmd::{Command};
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
                .failure()
                .stderr(predicates::str::contains("[MAIN_1]"));
            }

#[test]
fn line_comment_2() {
    let _cmd = Command::cargo_bin("roust")
                .unwrap()
                .arg("tests/line_comment_2.roust")
                .assert()
                .failure()
                .stderr(predicates::str::contains("[MAIN_1]"));
            }

#[test]
fn multi_comment_1() {
    let _cmd = Command::cargo_bin("roust")
                .unwrap()
                .arg("tests/multi_comment_1.roust")
                .assert()
                .failure()
                .stderr(predicates::str::contains("[MAIN_1]"));
            }

#[test]
fn multi_comment_2() {
    let _cmd = Command::cargo_bin("roust")
                .unwrap()
                .arg("tests/multi_comment_2.roust")
                .assert()
                .failure()
                .stderr(predicates::str::contains("[MAIN_1]"));
            }

#[test]
fn multi_comment_3() {
    let _cmd = Command::cargo_bin("roust")
                .unwrap()
                .arg("tests/multi_comment_3.roust")
                .assert()
                .failure()
                .stderr(predicates::str::contains("[MAIN_1]"));
            }

#[test]
fn empty_section_1() {
    let _cmd = Command::cargo_bin("roust")
                .unwrap()
                .arg("tests/empty_section_1.roust")
                .arg("-o empty_section_1.bin")
                .assert()
                .success();

    // Verify file is empty.  If so, then clean up.
    assert!(fs::read_to_string("empty_section_1.bin").unwrap().len() == 0);
    fs::remove_file("empty_section_1.bin").unwrap();

}

#[test]
fn simple_section_2() {
    let _cmd = Command::cargo_bin("roust")
                .unwrap()
                .arg("tests/simple_section_2.roust")
                .arg("-o simple_section_2.bin")
                .assert()
                .success();

    // Verify output file is correct.  If so, then clean up.
    assert_eq!("Wow!", fs::read_to_string("simple_section_2.bin").unwrap());
    fs::remove_file("simple_section_2.bin").unwrap();
}

#[test]
fn simple_section_3() {
    let _cmd = Command::cargo_bin("roust")
                .unwrap()
                .arg("tests/simple_section_3.roust")
                .arg("-o simple_section_3.bin")
                .assert()
                .success();

    // Verify output file is correct.  If so, then clean up.
    assert_eq!("Wow!Bye", fs::read_to_string("simple_section_3.bin").unwrap());
    fs::remove_file("simple_section_3.bin").unwrap();
}

#[test]
fn simple_section_4() {
    let _cmd = Command::cargo_bin("roust")
                .unwrap()
                .arg("tests/simple_section_4.roust")
                .arg("-o simple_section_4.bin")
                .assert()
                .success();

    // Verify output file is correct.  If so, then clean up.
    assert_eq!("Wow!\nBye", fs::read_to_string("simple_section_4.bin").unwrap());
    fs::remove_file("simple_section_4.bin").unwrap();
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
fn fuzz_found_1() {
    let _cmd = Command::cargo_bin("roust")
    .unwrap()
    .arg("tests/fuzz_found_2.roust")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[MAIN_2]"));
}

#[test]
fn fuzz_found_2() {
    let _cmd = Command::cargo_bin("roust")
    .unwrap()
    .arg("tests/fuzz_found_2.roust")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[AST_13]"));
}

#[test]
fn fuzz_found_3() {
    let _cmd = Command::cargo_bin("roust")
    .unwrap()
    .arg("tests/fuzz_found_3.roust")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[AST_13]"));
}

#[test]
fn fuzz_found_4() {
    let _cmd = Command::cargo_bin("roust")
    .unwrap()
    .arg("tests/fuzz_found_4.roust")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[AST_13]"));
}

#[test]
fn missing_brace_1() {
    let _cmd = Command::cargo_bin("roust")
    .unwrap()
    .arg("tests/missing_brace_1.roust")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[AST_14]"));
}

#[test]
fn nested_section_1() {
    let _cmd = Command::cargo_bin("roust")
                .unwrap()
                .arg("tests/nested_section_1.roust")
                .arg("-o nested_section_1.bin")
                .assert()
                .success();

    // Verify output file is correct.  If so, then clean up.
    assert_eq!("foo!\nBye\nbar!\nboo!\n", fs::read_to_string("nested_section_1.bin").unwrap());
    fs::remove_file("nested_section_1.bin").unwrap();
}

} // mod tests

