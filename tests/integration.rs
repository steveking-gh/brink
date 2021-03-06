#[cfg(test)]
mod tests {
use assert_cmd::{Command};
use std::fs;
use serial_test::serial;

// Many tests just use the default output file "output.bin".
// This creates a race condition since each test deletes this
// file when done.
// Use #[serial] on tests the produce output.bin to fix this race condition.

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
fn assert_7() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/assert_7.brink")
                .assert()
                .failure();
}

#[test]
fn assert_8() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/assert_8.brink")
                .arg("-o assert_8.bin")
                .assert()
                .success();

    // Verify output file is correct.  If so, then clean up.
    assert_eq!("Wow!", fs::read_to_string("assert_8.bin").unwrap());
    fs::remove_file("assert_8.bin").unwrap();
}

#[test]
fn assert_9() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/assert_9.brink")
                .arg("-o assert_9.bin")
                .assert()
                .success();

    // Verify output file is correct.  If so, then clean up.
    assert_eq!("Wow!", fs::read_to_string("assert_9.bin").unwrap());
    fs::remove_file("assert_9.bin").unwrap();
}

#[test]
fn assert_10() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/assert_10.brink")
                .arg("-o assert_10.bin")
                .assert()
                .success();

    // Verify output file is correct.  If so, then clean up.
    assert_eq!("Wow!", fs::read_to_string("assert_10.bin").unwrap());
    fs::remove_file("assert_10.bin").unwrap();
}

#[test]
fn assert_11() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/assert_11.brink")
                .assert()
                .failure();
}

#[test]
fn assert_12() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/assert_12.brink")
                .assert()
                .failure();
}

#[test]
fn assert_13() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/assert_13.brink")
                .arg("-o assert_13.bin")
                .assert()
                .success();

    // Verify output file is correct.  If so, then clean up.
    assert_eq!("Wow!", fs::read_to_string("assert_13.bin").unwrap());
    fs::remove_file("assert_13.bin").unwrap();
}

#[test]
fn assert_14() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/assert_14.brink")
                .arg("-o assert_14.bin")
                .assert()
                .success();

    // Verify output file is correct.  If so, then clean up.
    assert_eq!("Wow!", fs::read_to_string("assert_14.bin").unwrap());
    fs::remove_file("assert_14.bin").unwrap();
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
    .arg("tests/fuzz_found_1.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[PROC_1]"));
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
fn fuzz_found_8() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/fuzz_found_8.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[AST_20]"));
}

#[test]
fn fuzz_found_9() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/fuzz_found_9.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[AST_19]"));
}

#[test]
fn fuzz_found_10() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/fuzz_found_10.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[IR_3]"));
}

#[test]
fn fuzz_found_11() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/fuzz_found_11.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[AST_19]"));
}

#[test]
fn fuzz_found_12() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/fuzz_found_12.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[AST_19]"));
}

#[test]
fn fuzz_found_13() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/fuzz_found_13.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[EXEC_26]"));
}

#[test]
fn fuzz_found_14() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/fuzz_found_14.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[LINEAR_6]"));
}

#[test]
fn missing_brace_1() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/missing_brace_1.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[AST_3]")) // bad output
    .stderr(predicates::str::contains("[AST_14]")); // missing brace
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

#[test]
fn nested_section_2() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/nested_section_2.brink")
                .arg("-o nested_section_2.bin")
                .assert()
                .success();

    // Verify output file is correct.  If so, then clean up.
    assert_eq!("bar!\nbar!\n", fs::read_to_string("nested_section_2.bin").unwrap());
    fs::remove_file("nested_section_2.bin").unwrap();
}


#[test]
fn sizeof_1() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/sizeof_1.brink")
                .arg("-o sizeof_1.bin")
                .assert()
                .success();

    // Verify output file is correct.  If so, then clean up.
    assert_eq!("Wow!", fs::read_to_string("sizeof_1.bin").unwrap());
    fs::remove_file("sizeof_1.bin").unwrap();
}

#[test]
fn sizeof_2() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/sizeof_2.brink")
                .arg("-o sizeof_2.bin")
                .assert()
                .success();

    // Verify output file is correct.  If so, then clean up.
    assert_eq!("Wow!bar!\n", fs::read_to_string("sizeof_2.bin").unwrap());
    fs::remove_file("sizeof_2.bin").unwrap();
}

#[test]
fn sizeof_3() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/sizeof_3.brink")
                .arg("-o sizeof_3.bin")
                .assert()
                .success();

    // Verify output file is correct.  If so, then clean up.
    assert_eq!("Wow!", fs::read_to_string("sizeof_3.bin").unwrap());
    fs::remove_file("sizeof_3.bin").unwrap();
}

#[test]
#[serial]
fn integers_1() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/integers_1.brink")
                .assert()
                .success();

    fs::remove_file("output.bin").unwrap();
}

#[test]
#[serial]
fn integers_2() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/integers_2.brink")
                .assert()
                .success();

    fs::remove_file("output.bin").unwrap();
}

#[test]
#[serial]
fn integers_3() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/integers_3.brink")
                .assert()
                .success();

    fs::remove_file("output.bin").unwrap();
}

#[test]
fn integers_4() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/integers_4.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[AST_19]"));
}

#[test]
#[serial]
fn neq_1() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/neq_1.brink")
                .assert()
                .success();

    fs::remove_file("output.bin").unwrap();
}

#[test]
fn neq_2() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/neq_2.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[EXEC_2]"));
}

#[test]
#[serial]
fn add_1() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/add_1.brink")
                .assert()
                .success();

    fs::remove_file("output.bin").unwrap();
}

#[test]
fn add_2() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/add_2.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[EXEC_1]"));
}

#[test]
#[serial]
fn subtract_1() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/subtract_1.brink")
                .assert()
                .success();

    fs::remove_file("output.bin").unwrap();
}

#[test]
fn subtract_2() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/subtract_2.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[EXEC_4]"));
}

#[test]
fn subtract_3() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/subtract_3.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[EXEC_4]"));
}

#[test]
#[serial]
fn subtract_4() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/subtract_4.brink")
                .assert()
                .success();

    fs::remove_file("output.bin").unwrap();
}

#[test]
#[serial]
fn multiply_1() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/multiply_1.brink")
                .assert()
                .success();

    fs::remove_file("output.bin").unwrap();
}

#[test]
fn multiply_2() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/multiply_2.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[EXEC_6]"));
}

#[test]
#[serial]
fn divide_1() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/divide_1.brink")
                .assert()
                .success();

    fs::remove_file("output.bin").unwrap();
}

#[test]
#[serial]
fn shl_1() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/shl_1.brink")
                .assert()
                .success();

    fs::remove_file("output.bin").unwrap();
}

#[test]
#[serial]
fn shr_1() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/shr_1.brink")
                .assert()
                .success();

    fs::remove_file("output.bin").unwrap();
}

#[test]
#[serial]
fn bit_and_1() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/bit_and_1.brink")
                .assert()
                .success();

    fs::remove_file("output.bin").unwrap();
}

#[test]
#[serial]
fn bit_or_1() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/bit_or_1.brink")
                .assert()
                .success();

    fs::remove_file("output.bin").unwrap();
}

#[test]
#[serial]
fn geq_1() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/geq_1.brink")
                .assert()
                .success();

    fs::remove_file("output.bin").unwrap();
}

#[test]
#[serial]
fn leq_1() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/leq_1.brink")
                .assert()
                .success();

    fs::remove_file("output.bin").unwrap();
}

#[test]
#[serial]
fn logical_and_1() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/logical_and_1.brink")
                .assert()
                .success();

    fs::remove_file("output.bin").unwrap();
}

#[test]
#[serial]
fn logical_or_1() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/logical_or_1.brink")
                .assert()
                .success();

    fs::remove_file("output.bin").unwrap();
}

#[test]
#[serial]
fn address_1() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/address_1.brink")
                .assert()
                .success();

    fs::remove_file("output.bin").unwrap();
}

#[test]
#[serial]
fn address_2() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/address_2.brink")
                .assert()
                .success();

    fs::remove_file("output.bin").unwrap();
}

#[test]
#[serial]
fn address_3() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/address_3.brink")
                .assert()
                .success();

    fs::remove_file("output.bin").unwrap();
}

#[test]
fn address_4() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/address_4.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[LINEAR_6]"));
}


#[test]
fn address_5() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/address_5.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[LINEAR_7]"));
}

#[test]
fn address_6() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/address_6.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[LINEAR_6]"));
}

#[test]
fn address_7() {
    let _cmd = Command::cargo_bin("brink")
    .unwrap()
    .arg("tests/address_7.brink")
    .assert()
    .failure()
    .stderr(predicates::str::contains("[LINEAR_6]"));
}

#[test]
#[serial]
fn label_1() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/label_1.brink")
                .assert()
                .success();

    fs::remove_file("output.bin").unwrap();
}

#[test]
fn quoted_escapes_1() {
    let _cmd = Command::cargo_bin("brink")
                .unwrap()
                .arg("tests/quoted_escapes_1.brink")
                .arg("-o quoted_escapes_1.bin")
                .assert()
                .success();

    // Verify output file is correct.  If so, then clean up.
    //assert_eq!("Wow1\n\nWow2\tWos3\n\"Wow4\"\n\"Wow5\"Wo\"w6\"", fs::read_to_string("quoted_escapes_1.bin").unwrap());
    assert_eq!("Wow1\n\nWow2\tWow3\n\"Wow4\"\n\"Wow5\"Wo\"w6\"", fs::read_to_string("quoted_escapes_1.bin").unwrap());
    fs::remove_file("quoted_escapes_1.bin").unwrap();
}

} // mod tests

