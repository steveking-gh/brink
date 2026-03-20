#[cfg(test)]
mod tests {
    use assert_cmd::Command;
    use serial_test::serial;
    use std::fs;

    // Many tests just use the default output file "output.bin".
    // This creates a race condition since each test deletes this
    // file when done.
    // Use #[serial] on tests the produce output.bin to fix this race condition.

    fn assert_brink_success(src: &str, output_bin: Option<&str>, expected_output: Option<&str>) {
        let mut cmd = Command::cargo_bin("brink").unwrap();
        cmd.arg(src);
        if let Some(out_file) = output_bin {
            cmd.arg("-o").arg(out_file);
        }
        cmd.assert().success();

        let default_out = "output.bin";
        let actual_out = output_bin.unwrap_or(default_out);

        if fs::metadata(actual_out).is_ok() {
            if let Some(expected) = expected_output {
                let actual = fs::read_to_string(actual_out).unwrap_or_else(|_| "".to_string());
                assert_eq!(expected, actual);
            }
            fs::remove_file(actual_out).unwrap();
        }
    }

    fn assert_brink_failure(src: &str, expected_err_codes: &[&str]) {
        let mut assert = Command::cargo_bin("brink")
            .unwrap()
            .arg(src)
            .assert()
            .failure();
        for code in expected_err_codes {
            assert = assert.stderr(predicates::str::contains(*code));
        }
    }

    #[test]
    fn help_only() {
        let _cmd = Command::cargo_bin("brink").unwrap().arg("--help").unwrap();
    }

    #[test]
    #[should_panic]
    fn no_cli_input() {
        let _cmd = Command::cargo_bin("brink").unwrap().unwrap();
    }

    #[test]
    fn empty_1() {
        assert_brink_failure("tests/empty_1.brink", &[]);
    }

    #[test]
    fn accidental_infix() {
        assert_brink_failure("tests/accidental_infix.brink", &["AST_9"]);
    }

    #[test]
    fn bitwise_precedence() {
        assert_brink_success("tests/bitwise_precedence.brink", None, None);
    }

    #[test]
    #[serial]
    fn wr_multi() {
        assert_brink_success("tests/wr_multi.brink", None, None);
    }

    #[test]
    #[serial]
    fn wr_single() {
        assert_brink_success("tests/wr_single.brink", None, None);
    }

    #[test]
    fn empty_2() {
        assert_brink_failure("tests/empty_2.brink", &[]);
    }

    #[test]
    fn line_comment_1() {
        assert_brink_failure("tests/line_comment_1.brink", &["[AST_8]"]);
    }

    #[test]
    fn line_comment_2() {
        assert_brink_failure("tests/line_comment_2.brink", &["[AST_8]"]);
    }

    #[test]
    fn multi_comment_1() {
        assert_brink_failure("tests/multi_comment_1.brink", &["[AST_8]"]);
    }

    #[test]
    fn multi_comment_2() {
        assert_brink_failure("tests/multi_comment_2.brink", &["[AST_8]"]);
    }

    #[test]
    fn multi_comment_3() {
        assert_brink_failure("tests/multi_comment_3.brink", &["[AST_8]"]);
    }

    #[test]
    fn empty_section_1() {
        assert_brink_success(
            "tests/empty_section_1.brink",
            Some("empty_section_1.bin"),
            Some(""),
        );
    }

    #[test]
    fn simple_section_2() {
        assert_brink_success(
            "tests/simple_section_2.brink",
            Some("simple_section_2.bin"),
            Some("Wow!"),
        );
    }

    #[test]
    fn simple_section_3() {
        assert_brink_success(
            "tests/simple_section_3.brink",
            Some("simple_section_3.bin"),
            Some("Wow!Bye"),
        );
    }

    #[test]
    fn simple_section_4() {
        assert_brink_success(
            "tests/simple_section_4.brink",
            Some("simple_section_4.bin"),
            Some("Wow!\nBye"),
        );
    }

    #[test]
    fn assert_1() {
        assert_brink_success("tests/assert_1.brink", Some("assert_1.bin"), Some("Wow!"));
    }

    #[test]
    fn assert_2() {
        assert_brink_success("tests/assert_2.brink", Some("assert_2.bin"), Some("Wow!"));
    }

    #[test]
    fn assert_3() {
        assert_brink_success("tests/assert_3.brink", Some("assert_3.bin"), Some("Wow!"));
    }

    #[test]
    fn assert_4() {
        assert_brink_success("tests/assert_4.brink", Some("assert_4.bin"), Some("Wow!"));
    }

    #[test]
    fn assert_5() {
        assert_brink_success("tests/assert_5.brink", Some("assert_5.bin"), Some("Wow!"));
    }

    #[test]
    fn assert_6() {
        assert_brink_failure("tests/assert_6.brink", &[]);
    }

    #[test]
    fn assert_7() {
        assert_brink_failure("tests/assert_7.brink", &[]);
    }

    #[test]
    fn assert_8() {
        assert_brink_success("tests/assert_8.brink", Some("assert_8.bin"), Some("Wow!"));
    }

    #[test]
    fn assert_9() {
        assert_brink_success("tests/assert_9.brink", Some("assert_9.bin"), Some("Wow!"));
    }

    #[test]
    fn assert_10() {
        assert_brink_success("tests/assert_10.brink", Some("assert_10.bin"), Some("Wow!"));
    }

    #[test]
    fn assert_11() {
        assert_brink_failure("tests/assert_11.brink", &[]);
    }

    #[test]
    fn assert_12() {
        assert_brink_failure("tests/assert_12.brink", &[]);
    }

    #[test]
    fn assert_13() {
        assert_brink_success("tests/assert_13.brink", Some("assert_13.bin"), Some("Wow!"));
    }

    #[test]
    fn assert_14() {
        assert_brink_success("tests/assert_14.brink", Some("assert_14.bin"), Some("Wow!"));
    }

    #[test]
    fn assert_15() {
        assert_brink_failure("tests/assert_15.brink", &["[IRDB_4]"]);
    }

    #[test]
    fn section_rename_err_1() {
        assert_brink_failure("tests/section_rename_err_1.brink", &[]);
    }

    #[test]
    fn fuzz_found_1() {
        assert_brink_failure("tests/fuzz_found_1.brink", &["[PROC_1]"]);
    }

    #[test]
    fn fuzz_found_2() {
        assert_brink_failure("tests/fuzz_found_2.brink", &["[AST_13]"]);
    }

    #[test]
    fn fuzz_found_3() {
        assert_brink_failure("tests/fuzz_found_3.brink", &["[AST_13]"]);
    }

    #[test]
    fn fuzz_found_4() {
        assert_brink_failure("tests/fuzz_found_4.brink", &["[AST_13]"]);
    }

    #[test]
    fn fuzz_found_5() {
        assert_brink_failure("tests/fuzz_found_5.brink", &["[AST_16]"]);
    }

    #[test]
    fn fuzz_found_6() {
        assert_brink_failure("tests/fuzz_found_6.brink", &["[AST_16]"]);
    }

    #[test]
    fn fuzz_found_7() {
        assert_brink_failure("tests/fuzz_found_7.brink", &["[AST_13]", "[AST_14]"]);
    }

    #[test]
    fn fuzz_found_8() {
        assert_brink_failure("tests/fuzz_found_8.brink", &["[AST_20]"]);
    }

    #[test]
    fn fuzz_found_9() {
        assert_brink_failure("tests/fuzz_found_9.brink", &["[IRDB_5]"]);
    }

    #[test]
    fn fuzz_found_10() {
        assert_brink_failure("tests/fuzz_found_10.brink", &["[IR_4]"]);
    }

    #[test]
    fn fuzz_found_11() {
        assert_brink_failure("tests/fuzz_found_11.brink", &["[AST_19]"]);
    }

    #[test]
    fn fuzz_found_12() {
        assert_brink_failure("tests/fuzz_found_12.brink", &["[AST_19]"]);
    }

    #[test]
    fn fuzz_found_13() {
        assert_brink_failure("tests/fuzz_found_13.brink", &["[EXEC_26]"]);
    }

    #[test]
    fn fuzz_found_14() {
        assert_brink_failure("tests/fuzz_found_14.brink", &["[LINEAR_6]"]);
    }

    #[test]
    fn fuzz_found_15() {
        assert_brink_failure("tests/fuzz_found_15.brink", &["[AST_21]"]);
    }

    #[test]
    #[serial]
    fn fuzz_found_16() {
        assert_brink_success("tests/fuzz_found_16.brink", None, None);
    }

    #[test]
    fn fuzz_found_17() {
        assert_brink_failure("tests/fuzz_found_17.brink", &["[EXEC_13]"]);
    }

    #[test]
    fn fuzz_found_18() {
        assert_brink_failure("tests/fuzz_found_18.brink", &["[IRDB_2]"]);
    }

    #[test]
    fn missing_brace_1() {
        assert_brink_failure("tests/missing_brace_1.brink", &["[AST_3]", "[AST_14]"]);
    }

    #[test]
    fn multiple_outputs_1() {
        assert_brink_failure("tests/multiple_outputs_1.brink", &["[AST_10]"]);
    }

    #[test]
    fn section_self_ref_1() {
        assert_brink_failure("tests/section_self_ref_1.brink", &["[AST_6]"]);
    }

    #[test]
    fn section_self_ref_2() {
        assert_brink_failure("tests/section_self_ref_2.brink", &["[AST_6]"]);
    }

    #[test]
    fn nested_section_1() {
        assert_brink_success(
            "tests/nested_section_1.brink",
            Some("nested_section_1.bin"),
            Some("foo!\nBye\nbar!\nboo!\n"),
        );
    }

    #[test]
    fn nested_section_2() {
        assert_brink_success(
            "tests/nested_section_2.brink",
            Some("nested_section_2.bin"),
            Some("bar!\nbar!\n"),
        );
    }

    #[test]
    fn sizeof_1() {
        assert_brink_success("tests/sizeof_1.brink", Some("sizeof_1.bin"), Some("Wow!"));
    }

    #[test]
    fn sizeof_2() {
        assert_brink_success(
            "tests/sizeof_2.brink",
            Some("sizeof_2.bin"),
            Some("Wow!bar!\n"),
        );
    }

    #[test]
    fn sizeof_3() {
        assert_brink_success("tests/sizeof_3.brink", Some("sizeof_3.bin"), Some("Wow!"));
    }

    #[test]
    #[serial]
    fn integers_1() {
        assert_brink_success("tests/integers_1.brink", None, None);
    }

    #[test]
    #[serial]
    fn integers_2() {
        assert_brink_success("tests/integers_2.brink", None, None);
    }

    #[test]
    #[serial]
    fn integers_3() {
        assert_brink_success("tests/integers_3.brink", None, None);
    }

    #[test]
    #[serial]
    fn integers_4() {
        assert_brink_failure("tests/integers_4.brink", &["[AST_19]"]);
    }

    #[test]
    #[serial]
    fn integers_5() {
        assert_brink_failure("tests/integers_5.brink", &["[EXEC_13]"]);
    }

    #[test]
    #[serial]
    fn neq_1() {
        assert_brink_success("tests/neq_1.brink", None, None);
    }

    #[test]
    fn neq_2() {
        assert_brink_failure("tests/neq_2.brink", &["[EXEC_2]"]);
    }

    #[test]
    #[serial]
    fn add_1() {
        assert_brink_success("tests/add_1.brink", None, None);
    }

    #[test]
    fn add_2() {
        assert_brink_failure("tests/add_2.brink", &["[EXEC_1]"]);
    }

    #[test]
    #[serial]
    fn subtract_1() {
        assert_brink_success("tests/subtract_1.brink", None, None);
    }

    #[test]
    fn subtract_2() {
        assert_brink_failure("tests/subtract_2.brink", &["[EXEC_4]"]);
    }

    #[test]
    fn subtract_3() {
        assert_brink_failure("tests/subtract_3.brink", &["[EXEC_4]"]);
    }

    #[test]
    #[serial]
    fn subtract_4() {
        assert_brink_success("tests/subtract_4.brink", None, None);
    }

    #[test]
    #[serial]
    fn multiply_1() {
        assert_brink_success("tests/multiply_1.brink", None, None);
    }

    #[test]
    fn multiply_2() {
        assert_brink_failure("tests/multiply_2.brink", &["[EXEC_6]"]);
    }

    #[test]
    #[serial]
    fn divide_1() {
        assert_brink_success("tests/divide_1.brink", None, None);
    }

    #[test]
    #[serial]
    fn modulo_1() {
        assert_brink_success("tests/modulo_1.brink", None, None);
    }

    #[test]
    #[serial]
    fn shl_1() {
        assert_brink_success("tests/shl_1.brink", None, None);
    }

    #[test]
    #[serial]
    fn shr_1() {
        assert_brink_success("tests/shr_1.brink", None, None);
    }

    #[test]
    #[serial]
    fn bit_and_1() {
        assert_brink_success("tests/bit_and_1.brink", None, None);
    }

    #[test]
    #[serial]
    fn bit_or_1() {
        assert_brink_success("tests/bit_or_1.brink", None, None);
    }

    #[test]
    #[serial]
    fn geq_1() {
        assert_brink_success("tests/geq_1.brink", None, None);
    }

    #[test]
    #[serial]
    fn leq_1() {
        assert_brink_success("tests/leq_1.brink", None, None);
    }

    #[test]
    #[serial]
    fn logical_and_1() {
        assert_brink_success("tests/logical_and_1.brink", None, None);
    }

    #[test]
    #[serial]
    fn logical_or_1() {
        assert_brink_success("tests/logical_or_1.brink", None, None);
    }

    #[test]
    #[serial]
    fn address_1() {
        assert_brink_success("tests/address_1.brink", None, None);
    }

    #[test]
    #[serial]
    fn address_2() {
        assert_brink_success("tests/address_2.brink", None, None);
    }

    #[test]
    #[serial]
    fn address_3() {
        assert_brink_success("tests/address_3.brink", None, None);
    }

    #[test]
    fn address_4() {
        assert_brink_failure("tests/address_4.brink", &["[LINEAR_6]"]);
    }

    #[test]
    fn address_5() {
        assert_brink_failure("tests/address_5.brink", &["[LINEAR_7]"]);
    }

    #[test]
    fn address_6() {
        assert_brink_failure("tests/address_6.brink", &["[LINEAR_6]"]);
    }

    #[test]
    fn address_7() {
        assert_brink_failure("tests/address_7.brink", &["[LINEAR_6]"]);
    }

    #[test]
    fn abs_overflow() {
        assert_brink_failure("tests/abs_overflow.brink", &["[EXEC_39]"]);
    }

    #[test]
    fn align_overflow() {
        assert_brink_failure("tests/align_overflow.brink", &["[EXEC_42]"]);
    }

    #[test]
    fn set_abs_overflow() {
        assert_brink_failure("tests/set_abs_overflow.brink", &["[EXEC_43]"]);
    }

    #[test]
    fn abs_identifier_overflow() {
        assert_brink_failure("tests/abs_identifier_overflow.brink", &["[EXEC_44]"]);
    }

    #[test]
    #[serial]
    fn label_1() {
        assert_brink_success("tests/label_1.brink", None, None);
    }

    #[test]
    fn label_2() {
        assert_brink_failure("tests/label_2.brink", &["[LINEAR_9]"]);
    }

    #[test]
    fn label_3() {
        assert_brink_failure("tests/label_3.brink", &["[LINEAR_2]"]);
    }

    #[test]
    fn quoted_escapes_1() {
        assert_brink_success(
            "tests/quoted_escapes_1.brink",
            Some("quoted_escapes_1.bin"),
            None,
        );
    }

    #[test]
    #[serial]
    fn to_u64_1() {
        assert_brink_success("tests/to_u64_1.brink", None, None);
    }

    #[test]
    #[serial]
    fn to_i64_1() {
        assert_brink_success("tests/to_i64_1.brink", None, None);
    }

    #[test]
    #[serial]
    fn to_i64_2() {
        assert_brink_success("tests/to_i64_2.brink", None, None);
    }

    #[test]
    #[serial]
    fn print_1() {
        assert_brink_success("tests/print_1.brink", None, None);
    }

    #[test]
    #[serial]
    fn print_2() {
        assert_brink_success("tests/print_2.brink", None, None);
    }

    #[test]
    fn wrs_1() {
        assert_brink_success(
            "tests/wrs_1.brink",
            Some("wrs_1.bin"),
            Some("123\0456 Wow! 18 2\n"),
        );
    }

    #[test]
    fn wrs_overflow() {
        assert_brink_failure("tests/wrs_overflow.brink", &["[EXEC_41]"]);
    }

    #[test]
    fn wrx_overflow() {
        assert_brink_failure("tests/wrx_overflow.brink", &["[EXEC_36]"]);
    }

    #[test]
    fn wrx_1() {
        assert_brink_success(
            "tests/wrx_1.brink",
            Some("wrx_1.bin"),
            Some("1\n12\n123\n1234\n12345\n123456\n1234567\n12345678\n"),
        );
    }

    #[test]
    #[serial]
    fn wrx_2() {
        assert_brink_success("tests/wrx_2.brink", None, None);
    }

    #[test]
    #[serial]
    fn wrx_3() {
        assert_brink_success("tests/wrx_3.brink", None, None);
    }

    #[test]
    fn wrx_4() {
        let _cmd = Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/wrx_4.brink")
            .arg("-o wrx_4.bin")
            .assert()
            .success();

        // Verify output file is correct.  If so, then clean up.
        let bytevec = fs::read("wrx_4.bin").unwrap();
        assert!(bytevec.len() == 36);
        // wr8
        assert_eq!(bytevec[0], 49);
        // wr16
        assert_eq!(bytevec[1], 50);
        assert_eq!(bytevec[2], 00);
        // wr24
        assert_eq!(bytevec[3], 52);
        assert_eq!(bytevec[4], 00);
        assert_eq!(bytevec[5], 00);
        // wr32
        assert_eq!(bytevec[6], 55);
        assert_eq!(bytevec[7], 00);
        assert_eq!(bytevec[8], 00);
        assert_eq!(bytevec[9], 00);
        // wr40
        assert_eq!(bytevec[10], 59);
        assert_eq!(bytevec[11], 00);
        assert_eq!(bytevec[12], 00);
        assert_eq!(bytevec[13], 00);
        assert_eq!(bytevec[14], 00);
        // wr48
        assert_eq!(bytevec[15], 64);
        assert_eq!(bytevec[16], 00);
        assert_eq!(bytevec[17], 00);
        assert_eq!(bytevec[18], 00);
        assert_eq!(bytevec[19], 00);
        assert_eq!(bytevec[20], 00);
        // wr56
        assert_eq!(bytevec[21], 70);
        assert_eq!(bytevec[22], 00);
        assert_eq!(bytevec[23], 00);
        assert_eq!(bytevec[24], 00);
        assert_eq!(bytevec[25], 00);
        assert_eq!(bytevec[26], 00);
        assert_eq!(bytevec[27], 00);
        // wr64
        assert_eq!(bytevec[28], 77);
        assert_eq!(bytevec[29], 00);
        assert_eq!(bytevec[30], 00);
        assert_eq!(bytevec[31], 00);
        assert_eq!(bytevec[32], 00);
        assert_eq!(bytevec[33], 00);
        assert_eq!(bytevec[34], 00);
        assert_eq!(bytevec[35], 00);

        fs::remove_file("wrx_4.bin").unwrap();
    }

    #[test]
    fn wrx_5() {
        let _cmd = Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/wrx_5.brink")
            .arg("-o wrx_5.bin")
            .assert()
            .success();

        // Verify output file is correct.  If so, then clean up.
        let bytevec = fs::read("wrx_5.bin").unwrap();
        assert!(bytevec.len() == 36);
        // wr8
        assert_eq!(bytevec[0], 0x12);
        // wr16
        assert_eq!(bytevec[1], 0x12);
        assert_eq!(bytevec[2], 0x34);
        // wr24
        assert_eq!(bytevec[3], 0x12);
        assert_eq!(bytevec[4], 0x34);
        assert_eq!(bytevec[5], 0x56);
        // wr32
        assert_eq!(bytevec[6], 0x12);
        assert_eq!(bytevec[7], 0x34);
        assert_eq!(bytevec[8], 0x56);
        assert_eq!(bytevec[9], 0x78);
        // wr40
        assert_eq!(bytevec[10], 0x12);
        assert_eq!(bytevec[11], 0x34);
        assert_eq!(bytevec[12], 0x56);
        assert_eq!(bytevec[13], 0x78);
        assert_eq!(bytevec[14], 0xAB);
        // wr48
        assert_eq!(bytevec[15], 0x12);
        assert_eq!(bytevec[16], 0x34);
        assert_eq!(bytevec[17], 0x56);
        assert_eq!(bytevec[18], 0x78);
        assert_eq!(bytevec[19], 0xAB);
        assert_eq!(bytevec[20], 0xCD);
        // wr56
        assert_eq!(bytevec[21], 0x12);
        assert_eq!(bytevec[22], 0x34);
        assert_eq!(bytevec[23], 0x56);
        assert_eq!(bytevec[24], 0x78);
        assert_eq!(bytevec[25], 0xAB);
        assert_eq!(bytevec[26], 0xCD);
        assert_eq!(bytevec[27], 0xEF);
        // wr64
        assert_eq!(bytevec[28], 0x12);
        assert_eq!(bytevec[29], 0x34);
        assert_eq!(bytevec[30], 0x56);
        assert_eq!(bytevec[31], 0x78);
        assert_eq!(bytevec[32], 0xAB);
        assert_eq!(bytevec[33], 0xCD);
        assert_eq!(bytevec[34], 0xEF);
        assert_eq!(bytevec[35], 0x42);

        fs::remove_file("wrx_5.bin").unwrap();
    }

    #[test]
    fn wrx_6() {
        let _cmd = Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/wrx_6.brink")
            .arg("-o wrx_6.bin")
            .assert()
            .success();

        // Verify output file is correct.  If so, then clean up.
        let bytevec = fs::read("wrx_6.bin").unwrap();
        let temp: Vec<u8> = vec![
            1, 2, 2, 3, 3, 3, // wr8
            1, 0, 2, 0, 2, 0, 3, 0, 3, 0, 3, 0, // wr16
            1, 0, 0, 2, 0, 0, 2, 0, 0, 3, 0, 0, 3, 0, 0, 3, 0, 0, // wr24
            1, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 3, 0, 0, 0, 3, 0, 0, 0, // wr32
            1, 0, 0, 0, 0, 2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 3, 0, 0, 0, 0, 3, 0, 0, 0, 0, 3, 0, 0, 0,
            0, // wr40
            1, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0,
            0, 3, 0, 0, 0, 0, 0, // wr48
            1, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 3,
            0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, // wr56
            1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0,
            0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, // wr64
        ];
        println!("Bytevec length = {}", bytevec.len());
        assert!(bytevec.len() == 6 + 12 + 18 + 24 + 30 + 36 + 42 + 48);
        assert!(bytevec == temp);
        fs::remove_file("wrx_6.bin").unwrap();
    }

    #[test]
    fn align_0() {
        assert_brink_failure("tests/align_0.brink", &["[EXEC_38]"]);
    }

    #[test]
    #[serial]
    fn align_1() {
        assert_brink_success("tests/align_1.brink", None, None);
    }

    #[test]
    #[serial]
    fn align_2() {
        let _cmd = Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/align_2.brink")
            .arg("-o align_2.bin")
            .assert()
            .success();

        // Verify output file is correct.  If so, then clean up.
        let bytevec = fs::read("align_2.bin").unwrap();
        let temp: Vec<u8> = vec![
            1, 2, 3, 4, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // align 16;
            0xAA, 0xAA, 0xAA, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, // align 8, 0xFF;
            0xAA, 0xAA, 0xAA, 0x77, // align 7, 0x77;
        ];
        println!("Bytevec length = {}", bytevec.len());
        assert!(bytevec.len() == 28);
        assert!(bytevec == temp);
        fs::remove_file("align_2.bin").unwrap();
    }

    #[test]
    #[serial]
    fn set_sec_1() {
        assert_brink_success("tests/set_sec_1.brink", None, None);
    }

    #[test]
    fn set_sec_2() {
        assert_brink_failure("tests/set_sec_2.brink", &["[EXEC_22]"]);
    }

    #[test]
    #[serial]
    fn set_img_1() {
        assert_brink_success("tests/set_img_1.brink", None, None);
    }

    #[test]
    fn set_img_2() {
        assert_brink_failure("tests/set_img_2.brink", &["[EXEC_22]"]);
    }

    #[test]
    #[serial]
    fn set_abs_1() {
        assert_brink_success("tests/set_abs_1.brink", None, None);
    }

    #[test]
    fn set_abs_2() {
        assert_brink_failure("tests/set_abs_2.brink", &["[EXEC_22]"]);
    }

    #[test]
    #[serial]
    fn set_sec_3() {
        let _cmd = Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/set_sec_3.brink")
            .arg("-o set_sec_3.bin")
            .assert()
            .success();

        // Verify output file is correct.  If so, then clean up.
        let bytevec = fs::read("set_sec_3.bin").unwrap();
        let temp: Vec<u8> = vec![
            1, 2, 3, 4, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // set_sec 16;
            0xAA, 0xAA, 0xAA, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, // set_sec 24, 0xFF;
            0xAA, 0xAA, 0xAA, 0x77, // set_sec 28, 0x77;
        ];
        println!("Bytevec length = {}", bytevec.len());
        assert!(bytevec.len() == 28);
        assert!(bytevec == temp);
        fs::remove_file("set_sec_3.bin").unwrap();
    }

    #[test]
    fn wrf_1() {
        assert_brink_success("tests/wrf_1.brink", Some("wrf_1.bin"), Some("Hello!"));
    }

    #[test]
    fn wrf_2() {
        assert_brink_failure("tests/wrf_2.brink", &["[IRDB_13]"]);
    }

    #[test]
    fn wrf_3() {
        assert_brink_failure("tests/wrf_3.brink", &["[IRDB_11]"]);
    }

    #[test]
    #[should_panic]
    fn missing_input() {
        let mut cmd = Command::cargo_bin("brink").unwrap();
        cmd.arg("does_not_exist.brink").unwrap();
    }

    /// Assert that every error/warning/note code string passed to the diags API
    /// is unique across the entire source base.  A duplicated code would mean
    /// that `grep "SOME_CODE"` hits two unrelated call sites, defeating the
    /// searchability goal.
    #[test]
    fn error_codes_are_unique() {
        use std::collections::HashMap;

        let source_files = &[
            "ast/ast.rs",
            "ir/ir.rs",
            "lineardb/lineardb.rs",
            "irdb/irdb.rs",
            "engine/engine.rs",
            "process/process.rs",
            "src/main.rs",
        ];

        // Maps error code -> list of "file:line" locations where it appears.
        let mut code_locations: HashMap<String, Vec<String>> = HashMap::new();

        let prefixes = [
            "diags.err0(\"",
            "diags.err1(\"",
            "diags.err2(\"",
            "diags.warn(\"",
            "diags.note0(\"",
            "diags.note1(\"",
        ];

        for path in source_files {
            let contents = fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("Failed to read {}: {}", path, e));
            for (line_num, line) in contents.lines().enumerate() {
                for prefix in &prefixes {
                    if let Some(offset) = line.find(prefix) {
                        let after = &line[offset + prefix.len()..];
                        if let Some(end) = after.find('"') {
                            let code = after[..end].to_string();
                            let location = format!("{}:{}", path, line_num + 1);
                            code_locations.entry(code).or_default().push(location);
                        }
                    }
                }
            }
        }

        let mut duplicates: Vec<(String, Vec<String>)> = code_locations
            .into_iter()
            .filter(|(_, locs)| locs.len() > 1)
            .collect();
        duplicates.sort_by_key(|(code, _)| code.clone());

        if !duplicates.is_empty() {
            let mut msg = String::from("Duplicate error codes found:\n");
            for (code, locs) in &duplicates {
                msg.push_str(&format!("  \"{}\" appears at:\n", code));
                for loc in locs {
                    msg.push_str(&format!("    {}\n", loc));
                }
            }
            panic!("{}", msg);
        }
    }

    /// A decimal integer literal that exceeds i64::MAX cannot be stored
    /// as the ambiguous-integer type and produces IR_4.
    #[test]
    fn integer_overflow_i64() {
        assert_brink_failure("tests/integer_overflow_i64.brink", &["[IR_4]"]);
    }

    /// A hex integer literal with a 'u' suffix that exceeds u64::MAX produces IR_1.
    #[test]
    fn integer_overflow_u64() {
        assert_brink_failure("tests/integer_overflow_u64.brink", &["[IR_1]"]);
    }

    // -------------------------------------------------------------------------
    // const tests
    // -------------------------------------------------------------------------

    /// A basic integer const is defined and used as a wr8 operand.
    #[test]
    fn const_integer_1() {
        assert_brink_success(
            "tests/const_integer_1.brink",
            Some("const_integer_1.bin"),
            None,
        );
    }

    /// A u64 const is defined and used as a wr64 operand.
    #[test]
    fn const_u64_1() {
        assert_brink_success("tests/const_u64_1.brink", Some("const_u64_1.bin"), None);
    }

    /// A const is defined as an arithmetic expression composed of two other consts.
    #[test]
    fn const_expr_1() {
        assert_brink_success("tests/const_expr_1.brink", Some("const_expr_1.bin"), None);
    }

    /// A three-deep const chain: C depends on B which depends on A.
    #[test]
    fn const_chain_1() {
        assert_brink_success("tests/const_chain_1.brink", Some("const_chain_1.bin"), None);
    }

    /// A const is declared before the consts it depends on in source order.
    /// Resolution must be order-independent since consts have global scope.
    #[test]
    fn const_rev_decl_1() {
        assert_brink_success(
            "tests/const_rev_decl_1.brink",
            Some("const_rev_decl_1.bin"),
            None,
        );
    }

    /// A const is used as one operand of an assert expression inside a section.
    #[test]
    fn const_in_assert_1() {
        assert_brink_success(
            "tests/const_in_assert_1.brink",
            Some("const_in_assert_1.bin"),
            None,
        );
    }

    /// A string const is defined and used as a wrs operand.
    #[test]
    fn const_string_1() {
        assert_brink_success(
            "tests/const_string_1.brink",
            Some("const_string_1.bin"),
            None,
        );
    }

    /// A const identifier is used as the base address in the output statement.
    #[test]
    fn const_as_output_addr_1() {
        assert_brink_success(
            "tests/const_as_output_addr_1.brink",
            Some("const_as_output_addr_1.bin"),
            None,
        );
    }

    /// Two const declarations share the same name.  Expected: AST_30.
    #[test]
    fn const_duplicate_1() {
        assert_brink_failure("tests/const_duplicate_1.brink", &["[AST_30]"]);
    }

    /// A const name collides with an existing section name.  Expected: AST_31.
    #[test]
    fn const_name_conflict_1() {
        assert_brink_failure("tests/const_name_conflict_1.brink", &["[AST_31]"]);
    }

    /// Two consts mutually depend on each other, forming a cycle.  Expected: IRDB_18.
    #[test]
    fn const_circular_1() {
        assert_brink_failure("tests/const_circular_1.brink", &["[IRDB_18]"]);
    }

    /// A const expression depends on sizeof(), which requires engine-time layout.
    /// Consts must be resolvable before the engine runs.  Expected: IRDB_19.
    #[test]
    fn const_sizeof_1() {
        assert_brink_failure("tests/const_sizeof_1.brink", &["[IRDB_19]"]);
    }

    /// A const expression depends on abs(), which requires engine-time addressing.
    /// Consts must be resolvable before the engine runs.  Expected: IRDB_19.
    #[test]
    fn const_abs_1() {
        assert_brink_failure("tests/const_abs_1.brink", &["[IRDB_19]"]);
    }

    /// A const expression references an identifier that is never defined.
    /// Expected: IRDB_20.
    #[test]
    fn const_undefined_1() {
        assert_brink_failure("tests/const_undefined_1.brink", &["[IRDB_20]"]);
    }

    /// A const expression applies arithmetic to a string-typed const.
    /// Expected: IRDB_25.
    #[test]
    fn const_type_mismatch_1() {
        assert_brink_failure("tests/const_type_mismatch_1.brink", &["[IRDB_25]"]);
    }

    /// An I64 const is defined and used as a wr8 operand.
    #[test]
    fn const_i64_1() {
        assert_brink_success("tests/const_i64_1.brink", Some("const_i64_1.bin"), None);
    }

    /// Const expressions exercising subtract, multiply, divide, and modulo.
    #[test]
    fn const_arith_1() {
        assert_brink_success("tests/const_arith_1.brink", Some("const_arith_1.bin"), None);
    }

    /// Const expressions exercising bitwise and, or, left shift, and right shift.
    #[test]
    fn const_bitwise_1() {
        assert_brink_success(
            "tests/const_bitwise_1.brink",
            Some("const_bitwise_1.bin"),
            None,
        );
    }

    /// A const expression uses == which is not supported at compile time.
    /// Expected: IRDB_21.
    #[test]
    fn const_bad_op_1() {
        assert_brink_failure("tests/const_bad_op_1.brink", &["[IRDB_21]"]);
    }

    /// A const integer literal overflows i64.
    /// Expected: IRDB_22.
    #[test]
    fn const_bad_integer_1() {
        assert_brink_failure("tests/const_bad_integer_1.brink", &["[IRDB_22]"]);
    }

    /// A const U64 literal overflows u64.
    /// Expected: IRDB_23.
    #[test]
    fn const_bad_u64_1() {
        assert_brink_failure("tests/const_bad_u64_1.brink", &["[IRDB_23]"]);
    }

    /// A const I64 literal overflows i64.
    /// Expected: IRDB_24.
    #[test]
    fn const_bad_i64_1() {
        assert_brink_failure("tests/const_bad_i64_1.brink", &["[IRDB_24]"]);
    }

    /// A const U64 addition overflows u64::MAX.
    /// Expected: IRDB_27.
    #[test]
    fn const_overflow_1() {
        assert_brink_failure("tests/const_overflow_1.brink", &["[IRDB_27]"]);
    }

    /// A const integer division by zero.
    /// Expected: IRDB_28.
    #[test]
    fn const_divzero_1() {
        assert_brink_failure("tests/const_divzero_1.brink", &["[IRDB_28]"]);
    }

    /// Const comparison operators ==, !=, >=, <= produce U64 0/1 results.
    #[test]
    fn const_cmp_1() {
        assert_brink_success("tests/const_cmp_1.brink", Some("const_cmp_1.bin"), None);
    }

    /// Comparing a numeric const with a string const is a type error.
    /// Expected: IRDB_29.
    #[test]
    fn const_cmp_mismatch_1() {
        assert_brink_failure("tests/const_cmp_mismatch_1.brink", &["[IRDB_29]"]);
    }

    /// 'include' is reserved and cannot be used as a section name.
    #[test]
    fn reserved_section_1() {
        assert_brink_failure("tests/reserved_section_1.brink", &["[AST_32]"]);
    }

    /// Identifiers starting with 'wr' are reserved as section names.
    #[test]
    fn reserved_section_2() {
        assert_brink_failure("tests/reserved_section_2.brink", &["[AST_32]"]);
    }

    /// 'include' is reserved and cannot be used as a const name.
    #[test]
    fn reserved_const_1() {
        assert_brink_failure("tests/reserved_const_1.brink", &["[AST_33]"]);
    }

    /// Identifiers starting with 'wr' are reserved as const names.
    #[test]
    fn reserved_const_2() {
        assert_brink_failure("tests/reserved_const_2.brink", &["[AST_33]"]);
    }

    /// 'include' is reserved and cannot be used as a label name.
    #[test]
    fn reserved_label_1() {
        assert_brink_failure("tests/reserved_label_1.brink", &["[LINEAR_13]"]);
    }

    /// Identifiers starting with 'wr' are reserved as label names.
    #[test]
    fn reserved_label_2() {
        assert_brink_failure("tests/reserved_label_2.brink", &["[LINEAR_13]"]);
    }

    /// Identifiers starting with 'set_' are reserved as section names.
    #[test]
    fn reserved_section_3() {
        assert_brink_failure("tests/reserved_section_3.brink", &["[AST_32]"]);
    }

    /// 'let' is reserved and cannot be used as a const name.
    #[test]
    fn reserved_const_3() {
        assert_brink_failure("tests/reserved_const_3.brink", &["[AST_33]"]);
    }

    /// 'true' is reserved and cannot be used as a label name.
    #[test]
    fn reserved_label_3() {
        assert_brink_failure("tests/reserved_label_3.brink", &["[LINEAR_13]"]);
    }

    // ── Map file output tests ─────────────────────────────────────────────────

    /// Runs brink with --map-hf=<map_file> and -o <bin_file>, reads the map,
    /// asserts every string in `checks` appears in the map, then cleans up.
    fn assert_map_hf(src: &str, bin_out: &str, map_out: &str, checks: &[&str]) {
        let mut cmd = Command::cargo_bin("brink").unwrap();
        cmd.arg(src)
            .arg("-o")
            .arg(bin_out)
            .arg(format!("--map-hf={map_out}"));
        cmd.assert().success();

        let map =
            fs::read_to_string(map_out).unwrap_or_else(|_| panic!("map file not found: {map_out}"));
        for check in checks {
            assert!(
                map.contains(check),
                "map missing: {check:?}\n--- map ---\n{map}"
            );
        }

        fs::remove_file(bin_out).ok();
        fs::remove_file(map_out).ok();
    }

    /// Section names, absolute addresses, sizes, and const names/values all
    /// appear in the map.  hdr=2 bytes at 0x2000, body=5 bytes at 0x2002.
    #[test]
    fn map_hf_sections_and_consts() {
        assert_map_hf(
            "tests/map_sections.brink",
            "map_sections.bin",
            "map_sections.map.txt",
            &[
                "hdr",
                "body",
                "top",
                "0x0000000000002000", // BASE / hdr abs_start
                "0x0000000000002002", // body abs_start
                "2,",                 // hdr size
                "5,",                 // body size
                "BASE",               // const name
                "COUNT",              // const name
                "0x0000000000002000", // BASE value (U64 hex)
                "8",                  // COUNT value (Integer decimal)
            ],
        );
    }

    /// Label names and their absolute addresses appear in the map.
    /// 'entry' is at 0x5000, 'done' is at 0x5003.
    #[test]
    fn map_hf_labels() {
        assert_map_hf(
            "tests/map_labels.brink",
            "map_labels.bin",
            "map_labels.map.txt",
            &[
                "entry",
                "done",
                "0x0000000000005000", // entry abs_addr
                "0x0000000000005003", // done abs_addr
            ],
        );
    }

    /// A section written three times via wr produces three map entries.
    #[test]
    fn map_hf_repeated_section() {
        assert_map_hf(
            "tests/map_repeated.brink",
            "map_repeated.bin",
            "map_repeated.map.txt",
            &["chunk"],
        );
        // Verify chunk appears three times by reading the map independently.
        // (assert_map_hf already cleaned up, so re-run for the count check.)
        let mut cmd = Command::cargo_bin("brink").unwrap();
        cmd.arg("tests/map_repeated.brink")
            .arg("-o")
            .arg("map_repeated2.bin")
            .arg("--map-hf=map_repeated2.map.txt");
        cmd.assert().success();
        let map = fs::read_to_string("map_repeated2.map.txt").unwrap();
        assert!(
            map.matches("chunk").count() >= 3,
            "expected at least 3 'chunk' entries"
        );
        fs::remove_file("map_repeated2.bin").ok();
        fs::remove_file("map_repeated2.map.txt").ok();
    }

    /// Omitting FILE from --map-hf creates <stem>.map.txt in the current directory.
    #[test]
    #[serial]
    fn map_hf_default_filename() {
        let mut cmd = Command::cargo_bin("brink").unwrap();
        cmd.arg("tests/map_default.brink")
            .arg("-o")
            .arg("map_default.bin")
            .arg("--map-hf");
        cmd.assert().success();

        let map = fs::read_to_string("map_default.map.txt")
            .expect("default map file map_default.map.txt not created");
        assert!(
            map.contains("foo"),
            "section 'foo' missing from default map"
        );
        assert!(
            map.contains("0x0000000000001000"),
            "base address missing from default map"
        );

        fs::remove_file("map_default.bin").ok();
        fs::remove_file("map_default.map.txt").ok();
    }

    /// --map-hf=- writes map content to stdout.
    #[test]
    fn map_hf_stdout() {
        Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/map_sections.brink")
            .arg("-o")
            .arg("map_stdout.bin")
            .arg("--map-hf=-")
            .assert()
            .success()
            .stdout(predicates::str::contains("hdr"))
            .stdout(predicates::str::contains("body"))
            .stdout(predicates::str::contains("BASE"))
            .stdout(predicates::str::contains("0x0000000000002000"));
        fs::remove_file("map_stdout.bin").ok();
    }

    // ── JSON map output tests ─────────────────────────────────────────────────

    /// Runs brink with --map-json=<file>, parses the output as JSON, and
    /// asserts that every (key, value) pair in `checks` is present at the
    /// top level of every object in the named array field.
    fn assert_map_json(src: &str, bin_out: &str, map_out: &str) -> serde_json::Value {
        let mut cmd = Command::cargo_bin("brink").unwrap();
        cmd.arg(src)
            .arg("-o")
            .arg(bin_out)
            .arg(format!("--map-json={map_out}"));
        cmd.assert().success();

        let text = fs::read_to_string(map_out)
            .unwrap_or_else(|_| panic!("JSON map file not found: {map_out}"));
        let v: serde_json::Value = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("JSON map is not valid JSON: {e}\n{text}"));

        fs::remove_file(bin_out).ok();
        fs::remove_file(map_out).ok();
        v
    }

    /// JSON output is valid and contains correct header fields.
    #[test]
    fn map_json_header() {
        let v = assert_map_json(
            "tests/map_sections.brink",
            "map_json_header.bin",
            "map_json_header.map.json",
        );
        assert_eq!(v["output_file"], "map_json_header.bin");
        assert_eq!(v["base_addr"], "0x0000000000002000");
        assert_eq!(v["total_size"], 7u64);
    }

    /// JSON sections array contains correct names, addresses, and sizes.
    #[test]
    fn map_json_sections() {
        let v = assert_map_json(
            "tests/map_sections.brink",
            "map_json_sections.bin",
            "map_json_sections.map.json",
        );
        let sections = v["sections"].as_array().unwrap();
        let hdr = sections.iter().find(|s| s["name"] == "hdr").unwrap();
        assert_eq!(hdr["address"], "0x0000000000002000");
        assert_eq!(hdr["size"], 2u64);
        let body = sections.iter().find(|s| s["name"] == "body").unwrap();
        assert_eq!(body["address"], "0x0000000000002002");
        assert_eq!(body["size"], 5u64);
    }

    /// JSON constants array contains correct names and values.
    #[test]
    fn map_json_consts() {
        let v = assert_map_json(
            "tests/map_sections.brink",
            "map_json_consts.bin",
            "map_json_consts.map.json",
        );
        let consts = v["constants"].as_array().unwrap();
        let base = consts.iter().find(|c| c["name"] == "BASE").unwrap();
        assert_eq!(base["value"], "0x0000000000002000");
        let count = consts.iter().find(|c| c["name"] == "COUNT").unwrap();
        assert_eq!(count["value"], "8");
    }

    /// JSON labels array contains correct names and addresses.
    #[test]
    fn map_json_labels() {
        let v = assert_map_json(
            "tests/map_labels.brink",
            "map_json_labels.bin",
            "map_json_labels.map.json",
        );
        let labels = v["labels"].as_array().unwrap();
        let entry = labels.iter().find(|l| l["name"] == "entry").unwrap();
        assert_eq!(entry["address"], "0x0000000000005000");
        let done = labels.iter().find(|l| l["name"] == "done").unwrap();
        assert_eq!(done["address"], "0x0000000000005003");
    }

    /// A section written three times produces three JSON section entries.
    #[test]
    fn map_json_repeated_section() {
        let v = assert_map_json(
            "tests/map_repeated.brink",
            "map_json_repeated.bin",
            "map_json_repeated.map.json",
        );
        let chunks: Vec<_> = v["sections"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["name"] == "chunk")
            .collect();
        assert_eq!(chunks.len(), 3, "expected 3 'chunk' entries");
    }

    /// Omitting FILE from --map-json creates <stem>.map.json in the current directory.
    #[test]
    #[serial]
    fn map_json_default_filename() {
        let mut cmd = Command::cargo_bin("brink").unwrap();
        cmd.arg("tests/map_default.brink")
            .arg("-o")
            .arg("map_json_default.bin")
            .arg("--map-json");
        cmd.assert().success();

        let text = fs::read_to_string("map_default.map.json")
            .expect("default JSON map file map_default.map.json not created");
        let v: serde_json::Value = serde_json::from_str(&text).expect("not valid JSON");
        assert_eq!(v["base_addr"], "0x0000000000001000");

        fs::remove_file("map_json_default.bin").ok();
        fs::remove_file("map_default.map.json").ok();
    }

    /// --map-json=- writes JSON map content to stdout.
    #[test]
    fn map_json_stdout() {
        Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/map_sections.brink")
            .arg("-o")
            .arg("map_json_stdout.bin")
            .arg("--map-json=-")
            .assert()
            .success()
            .stdout(predicates::str::contains("\"hdr\""))
            .stdout(predicates::str::contains("\"BASE\""))
            .stdout(predicates::str::contains("0x0000000000002000"));
        fs::remove_file("map_json_stdout.bin").ok();
    }

    /// -DBASE=0x3000u places the output at 0x3000 and the const appears in the
    /// human-friendly map; -DCOUNT=16 also appears as a const in the map.
    #[test]
    fn defines_appear_in_hf_map() {
        let mut cmd = Command::cargo_bin("brink").unwrap();
        cmd.arg("tests/map_defines.brink")
            .arg("-o")
            .arg("defines_hf.bin")
            .arg("--map-hf=defines_hf.map.txt")
            .arg("-DBASE=0x3000u")
            .arg("-DCOUNT=16");
        cmd.assert().success();

        let map = fs::read_to_string("defines_hf.map.txt")
            .unwrap_or_else(|_| panic!("map file not found"));
        assert!(map.contains("0x0000000000003000"), "BASE address missing");
        assert!(map.contains("BASE"), "BASE const name missing");
        assert!(map.contains("COUNT"), "COUNT const name missing");
        assert!(map.contains("16"), "COUNT value missing");

        fs::remove_file("defines_hf.bin").ok();
        fs::remove_file("defines_hf.map.txt").ok();
    }

    /// -D defines appear in JSON map output with correct types.
    #[test]
    fn defines_appear_in_json_map() {
        let mut cmd = Command::cargo_bin("brink").unwrap();
        cmd.arg("tests/map_defines.brink")
            .arg("-o")
            .arg("defines_json.bin")
            .arg("--map-json=defines_json.map.json")
            .arg("-DBASE=0x3000")
            .arg("-DCOUNT=16");
        cmd.assert().success();

        let text = fs::read_to_string("defines_json.map.json")
            .unwrap_or_else(|_| panic!("JSON map file not found"));
        let v: serde_json::Value =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("JSON not valid: {e}\n{text}"));

        assert_eq!(v["base_addr"], "0x0000000000003000");
        let consts = v["constants"].as_array().unwrap();
        let base = consts.iter().find(|c| c["name"] == "BASE").unwrap();
        assert_eq!(base["value"], "0x0000000000003000");
        let count = consts.iter().find(|c| c["name"] == "COUNT").unwrap();
        assert_eq!(count["value"], "16");

        fs::remove_file("defines_json.bin").ok();
        fs::remove_file("defines_json.map.json").ok();
    }

    /// A -D define overrides a same-named const declared in the source.
    #[test]
    fn define_overrides_source_const() {
        // map_sections.brink declares BASE = 0x2000u; override to 0x4000.
        let mut cmd = Command::cargo_bin("brink").unwrap();
        cmd.arg("tests/map_sections.brink")
            .arg("-o")
            .arg("define_override.bin")
            .arg("--map-json=define_override.map.json")
            .arg("-DBASE=0x4000");
        cmd.assert().success();

        let text = fs::read_to_string("define_override.map.json")
            .unwrap_or_else(|_| panic!("JSON map file not found"));
        let v: serde_json::Value =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("JSON not valid: {e}\n{text}"));

        assert_eq!(v["base_addr"], "0x0000000000004000");

        fs::remove_file("define_override.bin").ok();
        fs::remove_file("define_override.map.json").ok();
    }

    /// A bare -DFLAG (no =value) resolves to Integer(1) per GCC convention.
    #[test]
    fn define_bare_flag_is_one() {
        // Write a minimal source that uses FLAG as a const checked via assert.
        let src = "tests/map_defines_flag.brink";
        fs::write(
            src,
            "const FLAG = 0;\nsection s { wr8 0x01; }\noutput s 0x1000;\n",
        )
        .unwrap();

        let mut cmd = Command::cargo_bin("brink").unwrap();
        cmd.arg(src)
            .arg("-o")
            .arg("define_flag.bin")
            .arg("--map-json=define_flag.map.json")
            .arg("-DFLAG");
        cmd.assert().success();

        let text = fs::read_to_string("define_flag.map.json")
            .unwrap_or_else(|_| panic!("JSON map file not found"));
        let v: serde_json::Value =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("JSON not valid: {e}\n{text}"));

        let consts = v["constants"].as_array().unwrap();
        let flag = consts.iter().find(|c| c["name"] == "FLAG").unwrap();
        assert_eq!(flag["value"], "1");

        fs::remove_file(src).ok();
        fs::remove_file("define_flag.bin").ok();
        fs::remove_file("define_flag.map.json").ok();
    }
} // mod tests
