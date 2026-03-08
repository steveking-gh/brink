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
        assert_brink_failure("tests/fuzz_found_9.brink", &["[AST_19]"]);
    }

    #[test]
    fn fuzz_found_10() {
        assert_brink_failure("tests/fuzz_found_10.brink", &["[IR_3]"]);
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
        assert_brink_failure("tests/wrf_3.brink", &["[AST_19]"]);
    }

    #[test]
    #[should_panic]
    fn missing_input() {
        let mut cmd = Command::cargo_bin("brink").unwrap();
        cmd.arg("does_not_exist.brink").unwrap();
    }
} // mod tests
