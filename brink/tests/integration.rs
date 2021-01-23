#[cfg(test)]
mod tests {
use assert_cmd::{Command};
use std::fs;

#[test]
fn help_only() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("--help")
                .unwrap();
}

#[test]
#[should_panic]
fn missing_input() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("does_not_exist.brink")
                .unwrap();
}

#[test]
#[should_panic]
fn no_cli_input() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .unwrap();
}

#[test]
fn empty_1() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/empty_1.brink")
                .assert()
                .failure();
}

#[test]
fn empty_2() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/empty_2.brink")
                .assert()
                .failure();
}

#[test]
fn line_comment_1() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/line_comment_1.brink")
                .assert()
                .failure()
                .stderr(predicates::str::contains("[AST_8]"));
            }

#[test]
fn line_comment_2() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/line_comment_2.brink")
                .assert()
                .failure()
                .stderr(predicates::str::contains("[AST_8]"));
            }

#[test]
fn multi_comment_1() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/multi_comment_1.brink")
                .assert()
                .failure()
                .stderr(predicates::str::contains("[AST_8]"));
            }

#[test]
fn multi_comment_2() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/multi_comment_2.brink")
                .assert()
                .failure()
                .stderr(predicates::str::contains("[AST_8]"));
            }

#[test]
fn multi_comment_3() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/multi_comment_3.brink")
                .assert()
                .failure()
                .stderr(predicates::str::contains("[AST_8]"));
            }

#[test]
fn empty_section_1() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/empty_section_1.brink")
                .arg("-o empty_section_1.bin")
                .assert()
                .success();

    // Verify file is empty.  If so, then clean up.
    assert!(fs::read_to_string("empty_section_1.bin").unwrap().len() == 0);
    fs::remove_file("empty_section_1.bin").unwrap();

}

#[test]
fn simple_section_2() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/simple_section_2.brink")
                .arg("-o simple_section_2.bin")
                .assert()
                .success();

    // Verify output file is correct.  If so, then clean up.
    assert_eq!("Wow!", fs::read_to_string("simple_section_2.bin").unwrap());
    fs::remove_file("simple_section_2.bin").unwrap();
}

#[test]
fn simple_section_3() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/simple_section_3.brink")
                .arg("-o simple_section_3.bin")
                .assert()
                .success();

    // Verify output file is correct.  If so, then clean up.
    assert_eq!("Wow!Bye", fs::read_to_string("simple_section_3.bin").unwrap());
    fs::remove_file("simple_section_3.bin").unwrap();
}

#[test]
fn simple_section_4() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/simple_section_4.brink")
                .arg("-o simple_section_4.bin")
                .assert()
                .success();

    // Verify output file is correct.  If so, then clean up.
    assert_eq!("Wow!\nBye", fs::read_to_string("simple_section_4.bin").unwrap());
    fs::remove_file("simple_section_4.bin").unwrap();
}

#[test]
fn assert_1() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/assert_1.brink")
                .arg("-o assert_1.bin")
                .assert()
                .success();

    // Verify output file is correct.  If so, then clean up.
    assert_eq!("Wow!", fs::read_to_string("assert_1.bin").unwrap());
    fs::remove_file("assert_1.bin").unwrap();
}

#[test]
fn assert_2() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/assert_2.brink")
                .arg("-o assert_2.bin")
                .assert()
                .success();

    // Verify output file is correct.  If so, then clean up.
    assert_eq!("Wow!", fs::read_to_string("assert_2.bin").unwrap());
    fs::remove_file("assert_2.bin").unwrap();
}

#[test]
fn assert_3() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/assert_3.brink")
                .arg("-o assert_3.bin")
                .assert()
                .success();

    // Verify output file is correct.  If so, then clean up.
    assert_eq!("Wow!", fs::read_to_string("assert_3.bin").unwrap());
    fs::remove_file("assert_3.bin").unwrap();
}

#[test]
fn assert_4() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/assert_4.brink")
                .arg("-o assert_4.bin")
                .assert()
                .success();

    // Verify output file is correct.  If so, then clean up.
    assert_eq!("Wow!", fs::read_to_string("assert_4.bin").unwrap());
    fs::remove_file("assert_4.bin").unwrap();
}

#[test]
fn assert_5() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/assert_5.brink")
                .arg("-o assert_5.bin")
                .assert()
                .success();

    // Verify output file is correct.  If so, then clean up.
    assert_eq!("Wow!", fs::read_to_string("assert_5.bin").unwrap());
    fs::remove_file("assert_5.bin").unwrap();
}

#[test]
fn assert_6() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/assert_6.brink")
                .assert()
                .failure();
}

#[test]
fn section_rename_err_1() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/section_rename_err_1.brink")
    .assert()
    .failure();
}

#[test]
fn fuzz_found_1() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/fuzz_found_2.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[MAIN_2]"));
}

#[test]
fn fuzz_found_2() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/fuzz_found_2.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[AST_13]"));
}

#[test]
fn fuzz_found_3() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/fuzz_found_3.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[AST_13]"));
}

#[test]
fn fuzz_found_4() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/fuzz_found_4.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[AST_13]"));
}

#[test]
fn fuzz_found_5() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/fuzz_found_5.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[AST_16]"));
}

#[test]
fn fuzz_found_6() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/fuzz_found_6.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[AST_16]"));
}

#[test]
fn fuzz_found_7() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/fuzz_found_7.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[AST_13]"))
    .stderr(predicates::str::contains("[AST_14]"));
}

#[test]
fn missing_brace_1() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/missing_brace_1.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[AST_14]"));
}

#[test]
fn multiple_outputs_1() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/multiple_outputs_1.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[AST_10]"));
}

#[test]
fn section_self_ref_1() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/section_self_ref_1.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[AST_6]"));
}

#[test]
fn section_self_ref_2() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/section_self_ref_2.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[AST_6]"));
}


#[test]
fn nested_section_1() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/nested_section_1.brink")
                .arg("-o nested_section_1.bin")
                .assert()
                .success();

    // Verify output file is correct.  If so, then clean up.
    assert_eq!("foo!\nBye\nbar!\nboo!\n", fs::read_to_string("nested_section_1.bin").unwrap());
    fs::remove_file("nested_section_1.bin").unwrap();
}

} // mod tests

