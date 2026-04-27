#[cfg(test)]
mod tests {
    use assert_cmd::Command;
    use predicates::prelude::PredicateBooleanExt;
    use std::fs;

    fn assert_brink_success(src: &str, output_bin: Option<&str>, expected_output: Option<&str>) {
        let mut cmd = Command::cargo_bin("brink").unwrap();
        cmd.arg(src);

        let derived_out;
        let actual_out = if let Some(out_file) = output_bin {
            cmd.arg("-o").arg(out_file);
            out_file
        } else {
            // Generate a locally unique binary filename like `tests_dynamic_wr.brink.bin`
            // avoiding `output.bin` race-conditions across the parallel testing harness entirely!
            derived_out = format!("{}.bin", src.replace('/', "_").replace('\\', "_"));
            cmd.arg("-o").arg(&derived_out);
            &derived_out
        };

        cmd.assert().success().stderr(predicates::str::is_empty());

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

    /// Asserts that brink succeeds and none of the specified codes appear on stderr.
    /// Runs with `-v` so that any warnings would be visible if present.
    fn assert_brink_no_warning(src: &str, absent_codes: &[&str]) {
        let derived_out = format!("{}.bin", src.replace('/', "_").replace('\\', "_"));
        let mut assert = Command::cargo_bin("brink")
            .unwrap()
            .arg("-v")
            .arg(src)
            .arg("-o")
            .arg(&derived_out)
            .assert()
            .success();
        for code in absent_codes {
            assert = assert.stderr(predicates::str::contains(*code).not());
        }
        if fs::metadata(&derived_out).is_ok() {
            fs::remove_file(&derived_out).unwrap();
        }
    }

    /// Asserts that brink succeeds and emits the specified warning codes on stderr.
    /// Runs with `-v` so that warnings are not suppressed.
    fn assert_brink_warning(src: &str, expected_warn_codes: &[&str]) {
        let derived_out = format!("{}.bin", src.replace('/', "_").replace('\\', "_"));
        let mut assert = Command::cargo_bin("brink")
            .unwrap()
            .arg("-v")
            .arg(src)
            .arg("-o")
            .arg(&derived_out)
            .assert()
            .success();
        for code in expected_warn_codes {
            assert = assert.stderr(predicates::str::contains(*code));
        }
        if fs::metadata(&derived_out).is_ok() {
            fs::remove_file(&derived_out).unwrap();
        }
    }

    #[test]
    fn help_only() {
        let _cmd = Command::cargo_bin("brink").unwrap().arg("--help").unwrap();
    }

    #[test]
    fn list_extensions_flag() {
        Command::cargo_bin("brink")
            .unwrap()
            .arg("--list-extensions")
            .assert()
            .success()
            .stdout(predicates::str::contains("std::crc32c"))
            .stdout(predicates::str::contains("std::sha256"))
            .stdout(predicates::str::contains("std::md5"))
            .stderr(predicates::str::is_empty());
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

    fn wr_multi() {
        assert_brink_success("tests/wr_multi.brink", None, None);
    }

    #[test]

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
    fn fuzz_found_19() {
        assert_brink_failure("tests/fuzz_found_19.brink", &["[PROC_7]"]);
    }

    #[test]
    fn fuzz_found_20() {
        assert_brink_failure("tests/fuzz_found_20.brink", &["[AST_42]"]);
    }

    #[test]
    fn fuzz_found_21() {
        assert_brink_failure("tests/fuzz_found_21.brink", &["[IRDB_55]"]);
    }

    #[test]
    fn fuzz_found_22() {
        assert_brink_failure("tests/fuzz_found_22.brink", &["[IRDB_56]"]);
    }

    #[test]
    fn fuzz_found_23() {
        assert_brink_failure("tests/fuzz_found_23.brink", &["[AST_43]"]);
    }

    #[test]
    fn fuzz_found_24() {
        assert_brink_failure("tests/fuzz_found_24.brink", &["[LINEAR_1]"]);
    }

    #[test]
    fn fuzz_found_25() {
        assert_brink_failure("tests/fuzz_found_25.brink", &["[IRDB_40]"]);
    }

    #[test]
    fn fuzz_found_26() {
        assert_brink_failure("tests/fuzz_found_26.brink", &["[EXEC_62]"]);
    }

    /// assert inside a top-level if body with a string condition panics -- latent bug.
    #[test]
    fn const_bool_string_assert() {
        assert_brink_failure("tests/const_bool_string_assert.brink", &["[IRDB_57]"]);
    }

    /// && with a string lhs panics -- latent bug.
    #[test]
    fn const_bool_string_and() {
        assert_brink_failure("tests/const_bool_string_and.brink", &["[IRDB_58]"]);
    }

    /// || with a string lhs panics -- latent bug.
    #[test]
    fn const_bool_string_or() {
        assert_brink_failure("tests/const_bool_string_or.brink", &["[IRDB_58]"]);
    }

    /// --max-output-size 0 rejects a 1-byte output.
    #[test]
    fn max_output_size_flag() {
        Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/wr_single.brink")
            .arg("--max-output-size")
            .arg("0")
            .assert()
            .failure()
            .stderr(predicates::str::contains("[PROC_7]"));
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

    /// __BRINK_VERSION_STRING can be written with wrs and used in print.
    #[test]
    fn version_string_1() {
        assert_brink_success("tests/version_string_1.brink", None, None);
    }

    /// __BRINK_VERSION_MAJOR/MINOR/PATCH are U64 values usable in expressions.
    #[test]
    fn version_numeric_1() {
        assert_brink_success("tests/version_numeric_1.brink", None, None);
    }

    /// Version builtins are usable in const expressions.
    #[test]
    fn version_in_const_1() {
        assert_brink_success("tests/version_in_const_1.brink", None, None);
    }

    /// Version builtins can be written into the output as wr8 operands.
    #[test]
    fn version_written_1() {
        assert_brink_success("tests/version_written_1.brink", None, None);
    }

    /// __OUTPUT_SIZE equals sizeof(output_section) and can be used in asserts.
    #[test]
    fn output_size_1() {
        assert_brink_success("tests/output_size_1.brink", None, None);
    }

    /// __OUTPUT_SIZE written into the output header as a 4-byte field.
    #[test]
    fn output_size_2() {
        assert_brink_success("tests/output_size_2.brink", None, None);
    }

    /// __OUTPUT_ADDR is zero with no region
    #[test]
    fn output_addr_2() {
        assert_brink_success("tests/output_addr_2.brink", None, None);
    }

    #[test]

    fn integers_1() {
        assert_brink_success("tests/integers_1.brink", None, None);
    }

    #[test]

    fn integers_2() {
        assert_brink_success("tests/integers_2.brink", None, None);
    }

    #[test]

    fn integers_3() {
        assert_brink_success("tests/integers_3.brink", None, None);
    }

    #[test]

    fn integers_4() {
        assert_brink_failure("tests/integers_4.brink", &["[AST_19]"]);
    }

    #[test]

    fn integers_5() {
        assert_brink_failure("tests/integers_5.brink", &["[EXEC_13]"]);
    }

    #[test]

    fn neq_1() {
        assert_brink_success("tests/neq_1.brink", None, None);
    }

    #[test]
    fn neq_2() {
        assert_brink_failure("tests/neq_2.brink", &["[EXEC_2]"]);
    }

    #[test]

    fn add_1() {
        assert_brink_success("tests/add_1.brink", None, None);
    }

    #[test]
    fn add_2() {
        assert_brink_failure("tests/add_2.brink", &["[EXEC_1]"]);
    }

    #[test]

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

    fn subtract_4() {
        assert_brink_success("tests/subtract_4.brink", None, None);
    }

    #[test]

    fn multiply_1() {
        assert_brink_success("tests/multiply_1.brink", None, None);
    }

    #[test]
    fn multiply_2() {
        assert_brink_failure("tests/multiply_2.brink", &["[EXEC_6]"]);
    }

    #[test]

    fn divide_1() {
        assert_brink_success("tests/divide_1.brink", None, None);
    }

    #[test]

    fn modulo_1() {
        assert_brink_success("tests/modulo_1.brink", None, None);
    }

    #[test]

    fn shl_1() {
        assert_brink_success("tests/shl_1.brink", None, None);
    }

    #[test]

    fn shr_1() {
        assert_brink_success("tests/shr_1.brink", None, None);
    }

    #[test]

    fn bit_and_1() {
        assert_brink_success("tests/bit_and_1.brink", None, None);
    }

    #[test]

    fn bit_or_1() {
        assert_brink_success("tests/bit_or_1.brink", None, None);
    }

    #[test]

    fn geq_1() {
        assert_brink_success("tests/geq_1.brink", None, None);
    }

    #[test]

    fn leq_1() {
        assert_brink_success("tests/leq_1.brink", None, None);
    }

    #[test]

    fn logical_and_1() {
        assert_brink_success("tests/logical_and_1.brink", None, None);
    }

    #[test]

    fn logical_or_1() {
        assert_brink_success("tests/logical_or_1.brink", None, None);
    }

    #[test]

    fn address_1() {
        assert_brink_success("tests/address_1.brink", None, None);
    }

    #[test]

    fn address_2() {
        assert_brink_success("tests/address_2.brink", None, None);
    }

    #[test]

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
        assert_brink_failure("tests/abs_overflow.brink", &["[EXEC_43]"]);
    }

    #[test]
    fn align_overflow() {
        assert_brink_failure("tests/align_overflow.brink", &["[EXEC_37]"]);
    }

    #[test]
    fn set_addr_overflow() {
        assert_brink_failure("tests/set_addr_overflow.brink", &["[EXEC_43]"]);
    }

    #[test]
    fn abs_identifier_overflow() {
        assert_brink_failure("tests/abs_identifier_overflow.brink", &["[EXEC_43]"]);
    }

    #[test]

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

    fn to_u64_1() {
        assert_brink_success("tests/to_u64_1.brink", None, None);
    }

    #[test]

    fn to_i64_1() {
        assert_brink_success("tests/to_i64_1.brink", None, None);
    }

    #[test]

    fn to_i64_2() {
        assert_brink_success("tests/to_i64_2.brink", None, None);
    }

    /// to_i64() in a const expression: verifies that eval_const_expr_r dispatches
    /// on op before reading operands, so the unary ToI64 ([input, output]) is
    /// correctly evaluated rather than causing a stack overflow.
    #[test]

    fn const_to_i64_const() {
        assert_brink_success("tests/const_to_i64_const.brink", None, None);
    }

    /// brink::logger() in a const expression is rejected with IRDB_21
    /// (ExtensionCall not supported in const expressions) now that eval_const_expr_r
    /// dispatches on op before reading operands.  Previously the name was
    /// accidentally caught by IRDB_20 before the dispatch reached eval_const_expr_r.
    #[test]

    fn const_ext_call_const() {
        assert_brink_failure("tests/const_ext_call_const.brink", &["[IRDB_21]"]);
    }

    #[test]

    fn print_1() {
        assert_brink_success("tests/print_1.brink", None, None);
    }

    #[test]

    fn print_2() {
        assert_brink_success("tests/print_2.brink", None, None);
    }

    #[test]
    fn wrs_1() {
        assert_brink_success(
            "tests/wrs_1.brink",
            Some("wrs_1.bin"),
            Some("123\x00456 Wow! 18 2\n"),
        );
    }

    #[test]
    fn wrs_overflow() {
        assert_brink_failure("tests/wrs_overflow.brink", &["[EXEC_37]"]);
    }

    #[test]
    fn wrx_overflow() {
        assert_brink_failure("tests/wrx_overflow.brink", &["[EXEC_36]"]);
    }

    /// brink::huge_ext has cached_size = usize::MAX.  After one preceding
    /// byte advances file_offset to 1, iterate_wrext attempts 1 + usize::MAX
    /// (as u64) which overflows and must emit EXEC_37.
    #[test]
    fn wrext_overflow() {
        assert_brink_failure("tests/wrext_overflow.brink", &["[EXEC_37]"]);
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

    fn wrx_2() {
        assert_brink_success("tests/wrx_2.brink", None, None);
    }

    #[test]

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
        // wr8:  3+0+addr(foo)+sizeof(foo) = 3+0+0+36 = 39
        assert_eq!(bytevec[0], 39);
        // wr16: 3+1+0+36 = 40
        assert_eq!(bytevec[1], 40);
        assert_eq!(bytevec[2], 00);
        // wr24: 3+3+0+36 = 42
        assert_eq!(bytevec[3], 42);
        assert_eq!(bytevec[4], 00);
        assert_eq!(bytevec[5], 00);
        // wr32: 3+6+0+36 = 45
        assert_eq!(bytevec[6], 45);
        assert_eq!(bytevec[7], 00);
        assert_eq!(bytevec[8], 00);
        assert_eq!(bytevec[9], 00);
        // wr40: 3+10+0+36 = 49
        assert_eq!(bytevec[10], 49);
        assert_eq!(bytevec[11], 00);
        assert_eq!(bytevec[12], 00);
        assert_eq!(bytevec[13], 00);
        assert_eq!(bytevec[14], 00);
        // wr48: 3+15+0+36 = 54
        assert_eq!(bytevec[15], 54);
        assert_eq!(bytevec[16], 00);
        assert_eq!(bytevec[17], 00);
        assert_eq!(bytevec[18], 00);
        assert_eq!(bytevec[19], 00);
        assert_eq!(bytevec[20], 00);
        // wr56: 3+21+0+36 = 60
        assert_eq!(bytevec[21], 60);
        assert_eq!(bytevec[22], 00);
        assert_eq!(bytevec[23], 00);
        assert_eq!(bytevec[24], 00);
        assert_eq!(bytevec[25], 00);
        assert_eq!(bytevec[26], 00);
        assert_eq!(bytevec[27], 00);
        // wr64: 3+28+0+36 = 67
        assert_eq!(bytevec[28], 67);
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

    fn align_1() {
        assert_brink_success("tests/align_1.brink", None, None);
    }

    #[test]

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

    fn set_sec_offset_1() {
        assert_brink_success("tests/set_sec_offset_1.brink", None, None);
    }

    #[test]
    fn set_sec_offset_2() {
        assert_brink_failure("tests/set_sec_offset_2.brink", &["[EXEC_22]"]);
    }

    #[test]

    fn set_addr_offset_1() {
        assert_brink_success("tests/set_addr_offset_1.brink", None, None);
    }

    #[test]
    fn set_addr_offset_2() {
        assert_brink_failure("tests/set_addr_offset_2.brink", &["[EXEC_22]"]);
    }

    #[test]

    fn set_addr_1() {
        assert_brink_success("tests/set_addr_1.brink", None, None);
    }

    #[test]
    fn set_addr_2() {
        assert_brink_success("tests/set_addr_2.brink", None, None);
    }

    #[test]
    fn set_sec_offset_after_set_addr() {
        assert_brink_warning("tests/set_sec_offset_after_set_addr.brink", &["[EXEC_54]"]);
    }

    #[test]
    fn set_addr_scope_restore() {
        // Child's set_addr must not leak addr_base or addr_offset into parent.
        assert_brink_success("tests/set_addr_scope_restore.brink", None, None);
    }

    #[test]
    fn set_addr_three_levels() {
        // Grandchild set_addr must not reach child or grandparent on exit.
        assert_brink_success("tests/set_addr_three_levels.brink", None, None);
    }

    #[test]
    fn set_addr_two_siblings() {
        // Second sibling must inherit restored parent addr state, not first sibling's.
        assert_brink_success("tests/set_addr_two_siblings.brink", None, None);
    }

    #[test]
    fn set_addr_repeated_section() {
        // Section with set_addr written twice; each invocation scoped independently.
        assert_brink_success("tests/set_addr_repeated_section.brink", None, None);
    }

    #[test]
    fn set_addr_empty_child() {
        // Empty child with set_addr writes 0 bytes; parent addr must not change.
        assert_brink_success("tests/set_addr_empty_child.brink", None, None);
    }

    #[test]
    fn set_addr_backward_child() {
        // Child set_addr to lower address; parent addr_base restored correctly.
        assert_brink_success("tests/set_addr_backward_child.brink", None, None);
    }

    #[test]
    fn set_addr_inherit_addr_offset() {
        // Child with no set_addr inherits and continues parent addr_offset.
        assert_brink_success("tests/set_addr_inherit_addr_offset.brink", None, None);
    }

    #[test]
    fn set_addr_multi_in_child() {
        // Child calls set_addr twice; parent sees only the total byte count.
        assert_brink_success("tests/set_addr_multi_in_child.brink", None, None);
    }

    #[test]
    fn output_addr_root_anchors_without_output_base_after_set_addr() {
        // __OUTPUT_ADDR should remain 0 when output has no base and first statement is set_addr.
        assert_brink_success("tests/output_addr_section_set_addr.brink", None, None);
    }

    #[test]
    fn output_addr_root_anchors_with_output_base_after_set_addr() {
        // __OUTPUT_ADDR remains 0 even when set_addr is used inside the section.
        assert_brink_success(
            "tests/output_addr_section_set_addr_with_output_base.brink",
            None,
            None,
        );
    }

    #[test]
    fn set_sec_offset_after_set_addr_no_warn() {
        // set_addr as the first statement keeps addr_offset and sec_offset in
        // sync, so EXEC_54 must not fire.
        assert_brink_no_warning(
            "tests/set_sec_offset_after_set_addr_no_warn.brink",
            &["[EXEC_54]"],
        );
    }

    #[test]

    fn set_sec_offset_3() {
        let _cmd = Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/set_sec_offset_3.brink")
            .arg("-o set_sec_offset_3.bin")
            .assert()
            .success();

        // Verify output file is correct.  If so, then clean up.
        let bytevec = fs::read("set_sec_offset_3.bin").unwrap();
        let temp: Vec<u8> = vec![
            1, 2, 3, 4, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // set_sec_offset 16;
            0xAA, 0xAA, 0xAA, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, // set_sec_offset 24, 0xFF;
            0xAA, 0xAA, 0xAA, 0x77, // set_sec_offset 28, 0x77;
        ];
        println!("Bytevec length = {}", bytevec.len());
        assert!(bytevec.len() == 28);
        assert!(bytevec == temp);
        fs::remove_file("set_sec_offset_3.bin").unwrap();
    }

    #[test]
    fn file_offset_1() {
        // Success: assert()-heavy test inside the .brink file verifies
        // monotonic file_offset, set_addr independence, set_file_offset
        // padding, and the identifier form file_offset(name).
        let _cmd = Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/file_offset_1.brink")
            .arg("-o file_offset_1.bin")
            .assert()
            .success();

        // Verify the binary layout matches the expected pad bytes.
        //   file[0..4]   "head"
        //   file[4..7]   "inn"
        //   file[7..16]  0xBB x9  (set_file_offset 16, 0xBB)
        //   file[16..20] "tail"
        let bytevec = fs::read("file_offset_1.bin").unwrap();
        let expected: Vec<u8> = vec![
            b'h', b'e', b'a', b'd', // "head"
            b'i', b'n', b'n', // "inn"
            0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, // 9 pad bytes
            b't', b'a', b'i', b'l', // "tail"
        ];
        assert_eq!(bytevec.len(), 20);
        assert_eq!(bytevec, expected);
        fs::remove_file("file_offset_1.bin").unwrap();
    }

    #[test]
    fn file_offset_overflow() {
        // set_file_offset backwards (target < current) must produce EXEC_22.
        assert_brink_failure("tests/file_offset_overflow.brink", &["[EXEC_22]"]);
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

    /// sizeof() works for both section names and extension names, and the
    /// two forms agree on the byte count when the extension fills the section.
    #[test]
    fn extension_sizeof() {
        assert_brink_success("tests/extension_sizeof.brink", None, None);
    }

    /// sizeof() with extension call syntax (arguments) is a compile error.
    #[test]
    fn sizeof_ext_name_with_args_fails() {
        assert_brink_failure("tests/extension_sizeof_fail.brink", &["[AST_40]"]);
    }

    /// Trying to use non-wr command on the extension output.
    #[test]
    fn extension_non_wr_1_fails() {
        assert_brink_failure("tests/extension_non_wr_1.brink", &["[EXEC_14]"]);
    }

    /// Trying to use non-wr command on the extension output.
    #[test]
    fn extension_non_wr_2_fails() {
        assert_brink_failure("tests/extension_non_wr_2.brink", &["[IRDB_9]"]);
    }

    /// A string const passed as an extension argument is valid at IRDb time but
    /// the extension rejects it at runtime with EXEC_47.
    #[test]
    fn extension_str_arg_fails() {
        assert_brink_failure("tests/extension_str_arg.brink", &["[EXEC_47]"]);
    }

    /// An integer followed by a string const as extension arguments; the extension
    /// rejects the string arg at runtime with EXEC_47.
    #[test]
    fn extension_int_str_arg_fails() {
        assert_brink_failure("tests/extension_int_str_arg.brink", &["[EXEC_47]"]);
    }

    /// sizeof(section) produces a numeric u64 and is valid as an extension
    /// argument.  brink::test_crc encodes sizeof(data)==4 as big-endian u32.
    /// Expected output: 4 data bytes then 0x00 0x00 0x00 0x04.
    #[test]
    fn extension_sizeof_arg() {
        let out = "tests_extension_sizeof_arg.brink.bin";
        Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/extension_sizeof_arg.brink")
            .arg("-o")
            .arg(out)
            .assert()
            .success();
        let bytes = fs::read(out).expect("output file missing");
        assert_eq!(
            bytes,
            vec![0x01, 0x02, 0x03, 0x04, 0x00, 0x00, 0x00, 0x04],
            "sizeof(data)==4 must appear big-endian in the CRC slot"
        );
        fs::remove_file(out).ok();
    }

    /// Eight literal integer arguments; brink::test_sum8 writes sum(1..8)==36
    /// as a little-endian u64.
    #[test]
    fn extension_8arg() {
        let out = "tests_extension_8arg.brink.bin";
        Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/extension_8arg.brink")
            .arg("-o")
            .arg(out)
            .assert()
            .success();
        let bytes = fs::read(out).expect("output file missing");
        assert_eq!(
            bytes,
            vec![36u8, 0, 0, 0, 0, 0, 0, 0],
            "sum of 1..8 must be 36 in little-endian u64"
        );
        fs::remove_file(out).ok();
    }

    /// All eight extension arguments are arithmetic expressions; the computed
    /// values match 1..8 so the sum and output are identical to extension_8arg.
    #[test]
    fn extension_8arg_expr() {
        let out = "tests_extension_8arg_expr.brink.bin";
        Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/extension_8arg_expr.brink")
            .arg("-o")
            .arg(out)
            .assert()
            .success();
        let bytes = fs::read(out).expect("output file missing");
        assert_eq!(
            bytes,
            vec![36u8, 0, 0, 0, 0, 0, 0, 0],
            "arithmetic expression args must evaluate identically to literals"
        );
        fs::remove_file(out).ok();
    }

    /// Form 3 (section-name): brink::increment receives the `top` section
    /// slice (16 bytes) and appends each byte + 1.  Total output: 32 bytes.
    #[test]
    fn execute_extension_increment() {
        Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/extension_increment.brink")
            .arg("-o")
            .arg("execute_extension_increment.bin")
            .assert()
            .success();

        let produced = fs::read("execute_extension_increment.bin")
            .expect("execute_extension_increment.bin not found");

        assert_eq!(produced.len(), 32, "Expected 32 bytes total");

        // First 16 bytes: 0x00-0x0F written by the section.
        for i in 0..16 {
            assert_eq!(produced[i], i as u8, "Data byte {i} mismatch");
        }
        // Next 16 bytes: the section slice incremented by 1.
        for i in 0..16 {
            assert_eq!(
                produced[16 + i],
                (i as u8).wrapping_add(1),
                "Incremented byte {i} mismatch"
            );
        }

        fs::remove_file("execute_extension_increment.bin").ok();
    }

    // Form 3 (section-name): brink::ranged_sum receives the entire `out`
    // section (4 data bytes + 8-byte extension slot) as its image slice.
    // Sum of 0x01+0x02+0x03+0x04 = 10; extension slot bytes are zero.
    // Total output: 4 data bytes + 8-byte sum = 12 bytes.
    #[test]
    fn execute_extension_section_sum() {
        Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/extension_section_sum.brink")
            .arg("-o")
            .arg("execute_extension_section_sum.bin")
            .assert()
            .success();

        let produced = fs::read("execute_extension_section_sum.bin")
            .expect("execute_extension_section_sum.bin not found");

        assert_eq!(
            produced.len(),
            12,
            "Expected 4 data bytes + 8-byte sum = 12 bytes"
        );

        assert_eq!(
            &produced[..4],
            &[0x01, 0x02, 0x03, 0x04],
            "Data bytes mismatch"
        );

        // Sum of 1+2+3+4 = 10 (extension slot zeros do not affect the sum).
        let sum = u64::from_le_bytes(produced[4..12].try_into().unwrap());
        assert_eq!(sum, 10, "Sum must be 10");

        fs::remove_file("execute_extension_section_sum.bin").ok();
    }

    /// A ranged extension called with the section-name form on an empty section
    /// succeeds when the extension accepts empty input.
    /// brink::ranged_sum returns 0.
    #[test]
    fn execute_extension_zero_length_section_success() {
        let out_path = "ext_zero_length_section_success.bin";
        Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/extension_zero_length_section_success.brink")
            .arg("-o")
            .arg(out_path)
            .assert()
            .success();
        let bytes = fs::read(out_path).expect("output file not found");
        assert_eq!(
            bytes,
            vec![0u8; 8],
            "zero-length section sum must be all zeros"
        );
        fs::remove_file(out_path).unwrap();
    }

    /// A ranged extension called with the section-name form on an empty section
    /// fails when the extension rejects empty input.
    #[test]
    fn execute_extension_zero_length_section_failure() {
        assert_brink_failure("tests/extension_zero_length_section_failure.brink", &[]);
    }

    /// Global asserts (outside any section) pass through the validation phase
    /// after all sections and extensions are fully written.
    #[test]
    fn global_assert_passes() {
        assert_brink_success("tests/global_assert.brink", None, None);
    }

    /// A global assert that evaluates false must emit EXEC_2.
    #[test]
    fn global_assert_fails() {
        assert_brink_failure("tests/global_assert_fail.brink", &["[EXEC_2]"]);
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
            "layoutdb/layoutdb.rs",
            "irdb/irdb.rs",
            "layout_phase/layout_phase.rs",
            "map_phase/map_phase.rs",
            "exec_phase/exec_phase.rs",
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
            "diags.warn1(\"",
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

    /// A const is defined as an arithmetic expression composed of two other consts.
    #[test]
    fn const_builtins_1() {
        assert_brink_success(
            "tests/const_builtins_1.brink",
            Some("const_builtins_1.bin"),
            None,
        );
    }

    /// A three-deep const chain: C depends on B which depends on A.
    #[test]
    fn const_chain_1() {
        assert_brink_success("tests/const_chain_1.brink", Some("const_chain_1.bin"), None);
    }

    #[test]
    fn const_deferred_assignment_1() {
        assert_brink_success(
            "tests/const_deferred_assignment_1.brink",
            Some("const_deferred_assignment_1.bin"),
            None,
        );
    }

    #[test]
    fn const_deferred_assignment_2() {
        assert_brink_success(
            "tests/const_deferred_assignment_2.brink",
            Some("const_deferred_assignment_2.bin"),
            None,
        );
    }

    #[test]
    fn const_deferred_assignment_3() {
        assert_brink_failure("tests/const_deferred_assignment_3.brink", &["[SYMTAB_4]"]);
    }

    #[test]
    fn const_deferred_assignment_4() {
        assert_brink_failure("tests/const_deferred_assignment_4.brink", &["[SYMTAB_4]"]);
    }

    /// A const is declared before the consts it depends on in source order.
    /// Resolution must be order-independent since consts have global scope.
    #[test]
    fn const_rev_decl_1() {
        assert_brink_failure("tests/const_rev_decl_1.brink", &["[IRDB_20]"]);
    }

    /// A malformed const expression.
    #[test]
    fn const_malformed_1() {
        assert_brink_failure("tests/const_malformed_1.brink", &["[AST_9]"]);
    }

    /// No dynamic built-ins allowed in const
    #[test]
    fn const_builtins_2() {
        assert_brink_failure("tests/const_builtins_2.brink", &["[IRDB_19]"]);
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

    /// A const identifier is used as the starting address via set_addr inside the section.
    #[test]
    fn const_as_output_addr_1() {
        assert_brink_success(
            "tests/const_as_output_addr_1.brink",
            Some("const_as_output_addr_1.bin"),
            None,
        );
    }

    /// Two const declarations share the same name.
    #[test]
    fn const_duplicate_1() {
        assert_brink_failure("tests/const_duplicate_1.brink", &["[AST_30]"]);
    }

    /// Two const declarations share the same name.
    #[test]
    fn const_duplicate_2() {
        assert_brink_failure("tests/const_duplicate_2.brink", &["[AST_30]"]);
    }

    /// A const name collides with an existing section name.
    #[test]
    fn const_name_conflict_1() {
        assert_brink_failure("tests/const_name_conflict_1.brink", &["[AST_31]"]);
    }

    /// Two consts mutually depend on each other, forming a cycle.
    #[test]
    fn const_circular_1() {
        assert_brink_failure("tests/const_circular_1.brink", &["[IRDB_20]"]);
    }

    /// Two consts mutually depend on each other, forming a cycle.
    #[test]
    fn const_circular_2() {
        assert_brink_failure("tests/const_circular_2.brink", &["[IRDB_20]"]);
    }

    /// A const expression depends on sizeof(), which requires engine-time layout.
    /// Consts must be resolvable before the engine runs.
    #[test]
    fn const_sizeof_1() {
        assert_brink_failure("tests/const_sizeof_1.brink", &["[IRDB_19]"]);
    }

    /// A const expression uses __OUTPUT_SIZE, which requires engine-time layout.
    /// Consts must be resolvable before the engine runs.
    #[test]
    fn output_builtin_const_fail() {
        assert_brink_failure("tests/output_builtin_const_fail.brink", &["[IRDB_19]"]);
    }

    /// A const expression depends on addr(), which requires engine-time addressing.
    /// Consts must be resolvable before the engine runs.
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

    /// A const is defined but never used anywhere in the program.
    /// Expected: SYMTAB_1 warning.
    #[test]
    fn const_unused_1() {
        assert_brink_warning("tests/const_unused_1.brink", &["[SYMTAB_1]"]);
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

    /// A const expression uses &&, which is now supported (added with if/else).
    #[test]
    fn const_bad_op_1() {
        assert_brink_success("tests/const_bad_op_1.brink", None, None);
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

    /// 'wrs' is a dedicated lexer token and cannot be used as a section name.
    /// The lexer rejects it before the reserved-identifier check fires.
    #[test]
    fn reserved_section_4() {
        assert_brink_failure("tests/reserved_section_4.brink", &[]);
    }

    /// 'wrf' is a dedicated lexer token and cannot be used as a section name.
    /// The lexer rejects it before the reserved-identifier check fires.
    #[test]
    fn reserved_section_5() {
        assert_brink_failure("tests/reserved_section_5.brink", &[]);
    }

    /// Identifiers starting with '__' are reserved as section names.
    #[test]
    fn reserved_section_6() {
        assert_brink_failure("tests/reserved_section_6.brink", &["[AST_32]"]);
    }

    /// 'let' is reserved and cannot be used as a const name.
    #[test]
    fn reserved_const_3() {
        assert_brink_failure("tests/reserved_const_3.brink", &["[AST_33]"]);
    }

    /// 'wrs' is a dedicated lexer token and cannot be used as a const name.
    /// The lexer rejects it before the reserved-identifier check fires.
    #[test]
    fn reserved_const_4() {
        assert_brink_failure("tests/reserved_const_4.brink", &[]);
    }

    /// Identifiers starting with '__' are reserved as const names.
    #[test]
    fn reserved_const_5() {
        assert_brink_failure("tests/reserved_const_5.brink", &["[AST_33]"]);
    }

    /// 'true' is reserved and cannot be used as a label name.
    #[test]
    fn reserved_label_3() {
        assert_brink_failure("tests/reserved_label_3.brink", &["[LINEAR_13]"]);
    }

    /// 'wrs' is a reserved exact keyword and cannot be used as a label name.
    #[test]
    fn reserved_label_4() {
        assert_brink_failure("tests/reserved_label_4.brink", &["[LINEAR_13]"]);
    }

    /// Identifiers starting with '__' are reserved as label names.
    #[test]
    fn reserved_label_5() {
        assert_brink_failure("tests/reserved_label_5.brink", &["[LINEAR_13]"]);
    }

    /// 'wr' without a following digit is now a valid identifier prefix.
    #[test]
    fn wr_prefix_now_valid() {
        assert_brink_success("tests/wr_prefix_now_valid.brink", None, None);
    }

    // ── Map file output tests ─────────────────────────────────────────────────

    /// Runs brink with --map-csv=<map_file> and -o <bin_file>, reads the map,
    /// asserts every string in `checks` appears in the map, then cleans up.
    fn assert_map_csv(src: &str, bin_out: &str, map_out: &str, checks: &[&str]) {
        let mut cmd = Command::cargo_bin("brink").unwrap();
        cmd.arg(src)
            .arg("-o")
            .arg(bin_out)
            .arg(format!("--map-csv={map_out}"));
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
    fn map_csv_sections_and_consts() {
        assert_map_csv(
            "tests/map_sections.brink",
            "map_sections.bin",
            "map_sections.map.csv",
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
    fn map_csv_labels() {
        assert_map_csv(
            "tests/map_labels.brink",
            "map_labels.bin",
            "map_labels.map.csv",
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
    fn map_csv_repeated_section() {
        assert_map_csv(
            "tests/map_repeated.brink",
            "map_repeated.bin",
            "map_repeated.map.csv",
            &["chunk"],
        );
        // Verify chunk appears three times by reading the map independently.
        // (assert_map_csv already cleaned up, so re-run for the count check.)
        let mut cmd = Command::cargo_bin("brink").unwrap();
        cmd.arg("tests/map_repeated.brink")
            .arg("-o")
            .arg("map_repeated2.bin")
            .arg("--map-csv=map_repeated2.map.csv");
        cmd.assert().success();
        let map = fs::read_to_string("map_repeated2.map.csv").unwrap();
        assert!(
            map.matches("chunk").count() >= 3,
            "expected at least 3 'chunk' entries"
        );
        fs::remove_file("map_repeated2.bin").ok();
        fs::remove_file("map_repeated2.map.csv").ok();
    }

    /// Omitting FILE from --map-csv creates <stem>.map.csv in the current directory.
    #[test]
    fn map_csv_default_filename() {
        let mut cmd = Command::cargo_bin("brink").unwrap();
        // Copy the test file locally so the default output map name doesn't collide
        // with other map tests reading the same map_default.brink input!
        fs::copy("tests/map_default.brink", "tests/map_csv_default.brink").unwrap();

        cmd.arg("tests/map_csv_default.brink")
            .arg("-o")
            .arg("map_csv_default.bin")
            .arg("--map-csv");
        cmd.assert().success();

        let map = fs::read_to_string("map_csv_default.map.csv")
            .expect("default map file map_csv_default.map.csv not created");
        assert!(
            map.contains("foo"),
            "section 'foo' missing from default map"
        );
        assert!(
            map.contains("0x0000000000001000"),
            "base address missing from default map"
        );

        fs::remove_file("map_csv_default.bin").ok();
        fs::remove_file("map_csv_default.map.csv").ok();
        fs::remove_file("tests/map_csv_default.brink").ok();
    }

    /// --map-csv=- writes map content to stdout.
    #[test]
    fn map_csv_stdout() {
        Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/map_sections.brink")
            .arg("-o")
            .arg("map_stdout.bin")
            .arg("--map-csv=-")
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
    fn map_json_default_filename() {
        let mut cmd = Command::cargo_bin("brink").unwrap();
        fs::copy("tests/map_default.brink", "tests/map_json_default.brink").unwrap();

        cmd.arg("tests/map_json_default.brink")
            .arg("-o")
            .arg("map_json_default.bin")
            .arg("--map-json");
        cmd.assert().success();

        let text = fs::read_to_string("map_json_default.map.json")
            .expect("default JSON map file map_json_default.map.json not created");
        let v: serde_json::Value = serde_json::from_str(&text).expect("not valid JSON");
        assert_eq!(v["base_addr"], "0x0000000000001000");

        fs::remove_file("map_json_default.bin").ok();
        fs::remove_file("map_json_default.map.json").ok();
        fs::remove_file("tests/map_json_default.brink").ok();
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
    /// CSV map; -DCOUNT=16 also appears as a const in the map.
    #[test]
    fn defines_appear_in_csv_map() {
        let mut cmd = Command::cargo_bin("brink").unwrap();
        cmd.arg("tests/map_defines.brink")
            .arg("-o")
            .arg("defines_csv.bin")
            .arg("--map-csv=defines_csv.map.csv")
            .arg("-DBASE=0x3000u")
            .arg("-DCOUNT=16");
        cmd.assert().success();

        let map = fs::read_to_string("defines_csv.map.csv")
            .unwrap_or_else(|_| panic!("map file not found"));
        assert!(map.contains("0x0000000000003000"), "BASE address missing");
        assert!(map.contains("BASE"), "BASE const name missing");
        assert!(map.contains("COUNT"), "COUNT const name missing");
        assert!(map.contains("16"), "COUNT value missing");

        fs::remove_file("defines_csv.bin").ok();
        fs::remove_file("defines_csv.map.csv").ok();
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
            "const FLAG = 0;\nsection s { set_addr 0x1000; wr8 0x01; }\noutput s;\n",
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
    // ── Include Directive tests ───────────────────────────────────────────────

    #[test]
    fn include_missing_string() {
        assert_brink_failure("tests/include_missing_string.brink", &["[AST_34]"]);
    }

    #[test]
    fn include_missing_semi() {
        assert_brink_failure("tests/include_missing_semi.brink", &["[AST_35]"]);
    }

    #[test]
    fn include_cycle() {
        assert_brink_failure("tests/include_cycle.brink", &["[AST_36]"]);
    }

    #[test]
    fn include_cycle_multi() {
        assert_brink_failure("tests/include_cycle_a.brink", &["[AST_36]"]);
    }

    #[test]
    fn include_missing_file() {
        assert_brink_failure("tests/include_missing_file.brink", &["[AST_37]"]);
    }

    #[test]
    fn include_success() {
        let mut cmd = Command::cargo_bin("brink").unwrap();
        cmd.arg("tests/include_success_main.brink")
            .arg("-o")
            .arg("include_success.bin");
        cmd.assert().success();

        let bytevec = fs::read("include_success.bin").unwrap();
        let temp: Vec<u8> = vec![0xAA, 0xBB];
        assert_eq!(bytevec, temp);
        fs::remove_file("include_success.bin").ok();
    }

    #[test]
    fn include_nested_success() {
        let mut cmd = Command::cargo_bin("brink").unwrap();
        cmd.arg("tests/include_nested_main.brink")
            .arg("-o")
            .arg("include_nested.bin");
        cmd.assert().success();

        let bytevec = fs::read("include_nested.bin").unwrap();
        let temp: Vec<u8> = vec![0x11, 0x22];
        assert_eq!(bytevec, temp);
        fs::remove_file("include_nested.bin").ok();
    }

    /// Omitting FILE from --map-c99 creates <stem>.map.h in the current directory.
    #[test]
    fn map_c99_default_filename() {
        let mut cmd = Command::cargo_bin("brink").unwrap();
        fs::copy("tests/map_default.brink", "tests/map_c99_default.brink").unwrap();

        cmd.arg("tests/map_c99_default.brink")
            .arg("-o")
            .arg("map_c99_default.bin")
            .arg("--map-c99");
        cmd.assert().success();

        let map = fs::read_to_string("map_c99_default.map.h")
            .expect("Failed to open expected default map file map_c99_default.map.h");

        assert!(map.contains("#define MAP_C99_DEFAULT_MAP_H"));
        assert!(map.contains("MAP_C99_DEFAULT_MAP_foo_ADDR"));
        assert!(map.contains("MAP_C99_DEFAULT_MAP_TOTAL_SIZE"));
        assert!(map.contains("ULL"));

        fs::remove_file("map_c99_default.bin").ok();
        fs::remove_file("map_c99_default.map.h").ok();
        fs::remove_file("tests/map_c99_default.brink").ok();
    }

    /// --map-c99=- writes map content to stdout.
    #[test]
    fn map_c99_stdout() {
        Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/map_sections.brink")
            .arg("-o")
            .arg("map_stdout.bin")
            .arg("--map-c99=-")
            .assert()
            .success()
            .stdout(predicates::str::contains("MAP_STDOUT_MAP_hdr_ADDR"))
            .stdout(predicates::str::contains("MAP_STDOUT_MAP_body_SIZE"))
            .stdout(predicates::str::contains("0x0000000000002000ULL"));
        fs::remove_file("map_stdout.bin").ok();
    }

    /// --map-c99=- correctly parses user-defined compilation labels.
    #[test]
    fn map_c99_labels() {
        Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/map_labels.brink")
            .arg("-o")
            .arg("map_labels.bin")
            .arg("--map-c99=-")
            .assert()
            .success()
            .stdout(predicates::str::contains("MAP_LABELS_MAP_entry_ADDR"))
            .stdout(predicates::str::contains("MAP_LABELS_MAP_done_ADDR"))
            .stdout(predicates::str::contains("0x0000000000005000ULL"))
            .stdout(predicates::str::contains("0x0000000000005003ULL"));
        fs::remove_file("map_labels.bin").ok();
    }

    #[test]
    fn map_rs_default_filename() {
        let mut cmd = Command::cargo_bin("brink").unwrap();
        fs::copy("tests/map_default.brink", "tests/map_rs_default.brink").unwrap();

        cmd.arg("tests/map_rs_default.brink")
            .arg("-o")
            .arg("map_rs_default.bin")
            .arg("--map-rs");
        cmd.assert().success();

        let map = fs::read_to_string("map_rs_default.map.rs")
            .expect("Failed to open expected default map file map_rs_default.map.rs");

        assert!(map.contains("pub mod map_rs_default_map {"));
        assert!(map.contains("pub const FOO_ADDR: u64 = "));
        assert!(map.contains("pub const TOTAL_SIZE: u64 = "));

        fs::remove_file("map_rs_default.bin").ok();
        fs::remove_file("map_rs_default.map.rs").ok();
        fs::remove_file("tests/map_rs_default.brink").ok();
    }

    /// --map-rs=- writes map content to stdout.
    #[test]
    fn map_rs_stdout() {
        Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/map_sections.brink")
            .arg("-o")
            .arg("map_rs_stdout.bin")
            .arg("--map-rs=-")
            .assert()
            .success()
            .stdout(predicates::str::contains("pub mod map_rs_stdout_map {"))
            .stdout(predicates::str::contains("pub const HDR_ADDR: u64 = "))
            .stdout(predicates::str::contains("pub const BODY_SIZE: u64 = "))
            .stdout(predicates::str::contains("0x0000000000002000;"));
        fs::remove_file("map_rs_stdout.bin").ok();
    }

    /// --map-rs=- correctly parses user-defined compilation labels.
    #[test]
    fn map_rs_labels() {
        Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/map_labels.brink")
            .arg("-o")
            .arg("map_rs_labels.bin")
            .arg("--map-rs=-")
            .assert()
            .success()
            .stdout(predicates::str::contains("pub mod map_rs_labels_map {"))
            .stdout(predicates::str::contains("pub const ENTRY_ADDR: u64 = "))
            .stdout(predicates::str::contains("pub const DONE_ADDR: u64 = "))
            .stdout(predicates::str::contains("0x0000000000005000;"))
            .stdout(predicates::str::contains("0x0000000000005003;"));
        fs::remove_file("map_rs_labels.bin").ok();
    }

    #[test]
    fn invalid_namespace() {
        assert_brink_failure("tests/invalid_namespace.brink", &["IRDB_40"]);
    }

    #[test]
    fn invalid_cli_define() {
        Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/map_default.brink")
            .arg("-D")
            .arg("=")
            .assert()
            .failure()
            .stderr(predicates::str::contains("Empty name in define"));

        Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/map_default.brink")
            .arg("-D")
            .arg("=42")
            .assert()
            .failure()
            .stderr(predicates::str::contains("Empty name in define"));
    }

    #[test]
    fn execute_extension_crc() {
        Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/extension.brink")
            .arg("-o")
            .arg("execute_extension_crc.bin")
            .assert()
            .success();

        let produced =
            fs::read("execute_extension_crc.bin").expect("execute_extension_crc.bin not found");
        let expected = vec![0xAA, 0xBB, 0xCC, 0xDD];
        assert_eq!(produced, expected, "Mismatch in CRC mock output");
        fs::remove_file("execute_extension_crc.bin").ok();
    }

    // ── Address overwrite detection (EXEC_55) ─────────────────────────────────

    /// Two sections placed at the exact same address — complete overlap.
    #[test]
    fn addr_overwrite_1() {
        assert_brink_failure("tests/addr_overwrite_1.brink", &["EXEC_55"]);
    }

    /// Second section starts inside the first section's range — partial overlap.
    #[test]
    fn addr_overwrite_2() {
        assert_brink_failure("tests/addr_overwrite_2.brink", &["EXEC_55"]);
    }

    /// Second section is entirely contained within the first — engulfed overlap.
    #[test]
    fn addr_overwrite_3() {
        assert_brink_failure("tests/addr_overwrite_3.brink", &["EXEC_55"]);
    }

    /// Two sections placed back-to-back with no gap — valid, no EXEC_55.
    #[test]
    fn addr_no_overwrite_1() {
        assert_brink_no_warning("tests/addr_no_overwrite_1.brink", &["EXEC_55"]);
    }

    /// Two sections with a gap between them — valid, no EXEC_55.
    #[test]
    fn addr_no_overwrite_2() {
        assert_brink_no_warning("tests/addr_no_overwrite_2.brink", &["EXEC_55"]);
    }

    // ── if/else tests ────────────────────────────────────────────────────────

    /// Condition true: then-branch value written to output.
    #[test]
    fn if_else_true_branch() {
        assert_brink_success("tests/if_else_true_branch.brink", None, None);
    }

    /// Condition false: else-branch value written to output.
    #[test]
    fn if_else_false_branch() {
        assert_brink_success("tests/if_else_false_branch.brink", None, None);
    }

    /// No else clause, condition true: assigned const used successfully.
    #[test]
    fn if_no_else() {
        assert_brink_success("tests/if_no_else.brink", None, None);
    }

    /// else-if chain: correct branch selected.
    #[test]
    fn if_else_if_chain() {
        assert_brink_success("tests/if_else_if_chain.brink", None, None);
    }

    /// String comparison in if condition.
    #[test]
    fn if_string_compare() {
        assert_brink_success("tests/if_string_compare.brink", None, None);
    }

    /// print and assert inside an if/else body.
    #[test]
    fn if_print_assert() {
        assert_brink_success("tests/if_print_assert.brink", None, None);
    }

    /// Bare assignment to an undeclared name emits SYMTAB_3.
    #[test]
    fn if_bare_assign_undeclared() {
        assert_brink_failure("tests/if_bare_assign_undeclared.brink", &["SYMTAB_3"]);
    }

    /// Declared-only const never assigned and never used: no SYMTAB_1 warning.
    #[test]
    fn if_unused_declared() {
        assert_brink_no_warning("tests/if_unused_declared.brink", &["SYMTAB_1"]);
    }

    /// A deferred assignment inside an if-block targets a const declared later
    /// in source.  The assignment must fail with SYMTAB_3 because the const
    /// has not been declared at the point the if-block executes.
    #[test]
    fn if_interleaved_order() {
        assert_brink_failure("tests/if_interleaved_order.brink", &["SYMTAB_3"]);
    }

    /// An if-block condition references a const defined later in source order.
    /// The const must not be visible to the if-block; expected IRDB_20.
    #[test]
    fn if_full_const_after_if() {
        assert_brink_failure("tests/if_full_const_after_if.brink", &["IRDB_20"]);
    }

    /// A top-level assert after an if-block that performs a deferred assignment
    /// must see the assigned value and pass.
    #[test]
    fn if_top_level_assert_phase() {
        assert_brink_success("tests/if_top_level_assert_phase.brink", None, None);
    }

    // ── IRDB error path coverage ──────────────────────────────────────────────

    /// IRDB_1: binary operator applied to two typed but incompatible operands
    /// (U64 and I64) in the layout IR; neither is the untyped Integer class so
    /// get_operand_data_type_r cannot reconcile them.
    #[test]
    fn irdb_1_type_mismatch() {
        assert_brink_failure("tests/irdb_1_type_mismatch.brink", &["[IRDB_1]"]);
    }

    /// IRDB_9: wr8 operand is a quoted string rather than a numeric value.
    /// validate_numeric_1_or_2 must reject the non-integer first operand.
    #[test]
    fn irdb_9_wr_bad_type() {
        assert_brink_failure("tests/irdb_9_wr_bad_type.brink", &["[IRDB_9]"]);
    }

    /// IRDB_14: wrf path exists but is a directory, not a regular file.
    /// The is_file() check in validate_wrf_operands must reject it.
    #[test]
    fn irdb_14_wrf_dir() {
        assert_brink_failure("tests/irdb_14_wrf_dir.brink", &["[IRDB_14]"]);
    }

    /// IRDB_26: arithmetic operator applied to a non-numeric type (QuotedString)
    /// in a const expression.  apply_binary_op catches string operands in the
    /// final match arm after the reconciliation step passes.
    #[test]
    fn irdb_26_const_string_arith() {
        assert_brink_failure("tests/irdb_26_const_string_arith.brink", &["[IRDB_26]"]);
    }

    /// IRDB_30: ordered comparison (>=) applied to two string operands in a
    /// const expression.  Strings support only == and !=; apply_comparison_op
    /// must reject >= with IRDB_30.
    #[test]
    fn irdb_30_const_str_ordered_cmp() {
        assert_brink_failure("tests/irdb_30_const_str_ordered_cmp.brink", &["[IRDB_30]"]);
    }

    /// IRDB_32: assert expression evaluates to false inside a const if/else body.
    /// exec_const_statements must emit IRDB_32 when the assert condition is 0.
    #[test]
    fn irdb_32_const_assert_fails() {
        assert_brink_failure("tests/irdb_32_const_assert_fails.brink", &["[IRDB_32]"]);
    }

    /// IRDB_44: sizeof() applied to a namespace-qualified name not in the
    /// extension registry.  validate_operands must reject unknown SizeofExt names.
    #[test]
    fn irdb_44_unknown_sizeof_ext() {
        assert_brink_failure("tests/irdb_44_unknown_sizeof_ext.brink", &["[IRDB_44]"]);
    }

    /// A Slice-kinded extension parameter receives a quoted string instead of a
    /// section name.  IRDb rejects the call with IRDB_52.
    #[test]
    fn irdb_46_ranged_ext_bad_range() {
        assert_brink_failure("tests/irdb_46_ranged_ext_bad_range.brink", &["[IRDB_52]"]);
    }

    // ── Named extension arguments ─────────────────────────────────────────────

    /// Named Int argument: brink::test_crc(value=42) encodes 42 as big-endian u32.
    /// Expected output: 0x00 0x00 0x00 0x2A.
    #[test]
    fn named_arg_crc() {
        let out = "tests_named_arg_crc.brink.bin";
        Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/named_arg_crc.brink")
            .arg("-o")
            .arg(out)
            .assert()
            .success();
        let bytes = fs::read(out).expect("output file missing");
        assert_eq!(
            bytes,
            vec![0x00, 0x00, 0x00, 0x2A],
            "42 must encode as big-endian u32"
        );
        fs::remove_file(out).ok();
    }

    /// Named Slice argument to brink::test_increment.
    /// data=top passes the top section slice; execute appends each byte + 1.
    /// Expected output: 32 bytes -- 0x00-0x0F then 0x01-0x10.
    #[test]
    fn named_arg_section() {
        let out = "tests_named_arg_section.brink.bin";
        Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/named_arg_section.brink")
            .arg("-o")
            .arg(out)
            .assert()
            .success();
        let bytes = fs::read(out).expect("output file missing");
        assert_eq!(bytes.len(), 32, "expected 32 bytes total");
        for i in 0..16usize {
            assert_eq!(bytes[i], i as u8, "data byte {i} mismatch");
            assert_eq!(
                bytes[16 + i],
                (i as u8).wrapping_add(1),
                "incremented byte {i} mismatch"
            );
        }
        fs::remove_file(out).ok();
    }

    /// Named arguments supplied in reverse declaration order.
    /// h=8, g=7, ... a=1 must be reordered to (a..h) before execute.
    /// Expected output: sum(1..8) = 36 as little-endian u64.
    #[test]
    fn named_arg_sum8_reorder() {
        let out = "tests_named_arg_sum8_reorder.brink.bin";
        Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/named_arg_sum8_reorder.brink")
            .arg("-o")
            .arg(out)
            .assert()
            .success();
        let bytes = fs::read(out).expect("output file missing");
        assert_eq!(
            bytes,
            vec![36u8, 0, 0, 0, 0, 0, 0, 0],
            "sum of 1..8 must be 36 in little-endian u64"
        );
        fs::remove_file(out).ok();
    }

    /// Mixed positional and named arguments in one call: AST rejects with AST_40.
    #[test]
    fn ast_40_mixed_args() {
        assert_brink_failure("tests/ast_40_mixed_args.brink", &["[AST_40]"]);
    }

    /// Named argument with no value after '=': AST rejects with AST_41.
    #[test]
    fn ast_41_empty_rhs() {
        assert_brink_failure("tests/ast_41_empty_rhs.brink", &["[AST_41]"]);
    }

    /// Named argument with an unrecognized parameter name: IRDb rejects with IRDB_48.
    #[test]
    fn irdb_48_unknown_param() {
        assert_brink_failure("tests/irdb_48_unknown_param.brink", &["[IRDB_48]"]);
    }

    /// The same named parameter appears twice in one call: IRDb rejects with IRDB_49.
    #[test]
    fn irdb_49_dup_param() {
        assert_brink_failure("tests/irdb_49_dup_param.brink", &["[IRDB_49]"]);
    }

    /// Named-arg call missing required parameters: IRDb emits IRDB_51 for each absent param.
    #[test]
    fn irdb_51_missing_param() {
        assert_brink_failure("tests/irdb_51_missing_param.brink", &["[IRDB_51]"]);
    }

    /// Positional call with wrong argument count: IRDb rejects with IRDB_53.
    #[test]
    fn irdb_53_positional_count() {
        assert_brink_failure("tests/irdb_53_positional_count.brink", &["[IRDB_53]"]);
    }

    /// Positional Slice argument names a section that does not exist: IRDb rejects with IRDB_54.
    #[test]
    fn irdb_54_unknown_section() {
        assert_brink_failure("tests/irdb_54_unknown_section.brink", &["[IRDB_54]"]);
    }

    /// IRDB_15: wr repeat-count operand is a quoted string, not a numeric value.
    /// validate_numeric_1_or_2 rejects a non-integer second operand.
    #[test]
    fn irdb_15_wr_bad_repeat_type() {
        assert_brink_failure("tests/irdb_15_wr_bad_repeat_type.brink", &["[IRDB_15]"]);
    }

    // ── Section-level if/else (Phase 1: runtime statements in const conditions) ──

    /// True branch of a section-level if/else emits 0x41 ('A').
    #[test]
    fn section_if_true() {
        let _cmd = Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/section_if_true.brink")
            .arg("-o")
            .arg("section_if_true.bin")
            .assert()
            .success();
        let bytevec = fs::read("section_if_true.bin").unwrap();
        assert_eq!(bytevec, vec![0x41u8]);
        fs::remove_file("section_if_true.bin").unwrap();
    }

    /// False branch of a section-level if/else emits 0x42 ('B').
    #[test]
    fn section_if_false() {
        let _cmd = Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/section_if_false.brink")
            .arg("-o")
            .arg("section_if_false.bin")
            .assert()
            .success();
        let bytevec = fs::read("section_if_false.bin").unwrap();
        assert_eq!(bytevec, vec![0x42u8]);
        fs::remove_file("section_if_false.bin").unwrap();
    }

    /// Section-level if with no else and a false condition produces one byte (from hdr).
    #[test]
    fn section_if_no_else() {
        let _cmd = Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/section_if_no_else.brink")
            .arg("-o")
            .arg("section_if_no_else.bin")
            .assert()
            .success();
        let bytevec = fs::read("section_if_no_else.bin").unwrap();
        // hdr = [0xFF], payload is empty (WRITE_IT == 0), combo = hdr ++ payload
        assert_eq!(bytevec, vec![0xFFu8]);
        fs::remove_file("section_if_no_else.bin").unwrap();
    }

    /// else-if chain selects the correct branch (VAL == 2 → 0x32).
    #[test]
    fn section_if_else_if() {
        let _cmd = Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/section_if_else_if.brink")
            .arg("-o")
            .arg("section_if_else_if.bin")
            .assert()
            .success();
        let bytevec = fs::read("section_if_else_if.bin").unwrap();
        assert_eq!(bytevec, vec![0x32u8]);
        fs::remove_file("section_if_else_if.bin").unwrap();
    }

    /// True branch with multiple statements emits all three bytes.
    #[test]
    fn section_if_multi_stmt() {
        let _cmd = Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/section_if_multi_stmt.brink")
            .arg("-o")
            .arg("section_if_multi_stmt.bin")
            .assert()
            .success();
        let bytevec = fs::read("section_if_multi_stmt.bin").unwrap();
        assert_eq!(bytevec, vec![0x01u8, 0x02, 0x03]);
        fs::remove_file("section_if_multi_stmt.bin").unwrap();
    }

    // ── Section-level if/else: corner cases ──────────────────────────────────

    /// Corner case 1: nested if inside a section if.
    /// The prune loop must re-scan after promoting the inner if node.
    /// Outer true, inner false → else emits 0x02.
    #[test]
    fn section_if_nested() {
        let _cmd = Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/section_if_nested.brink")
            .arg("-o")
            .arg("section_if_nested.bin")
            .assert()
            .success();
        let bytevec = fs::read("section_if_nested.bin").unwrap();
        assert_eq!(bytevec, vec![0x02u8]);
        fs::remove_file("section_if_nested.bin").unwrap();
    }

    /// Corner case 2: `wr <section>` inside a section if.
    /// After pruning the promoted wr node must still be a valid LayoutDb section write.
    /// Expected output: 0xDE 0xAD (from header) then 0xBE 0xEF (literal bytes).
    #[test]
    fn section_if_wr_section() {
        let _cmd = Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/section_if_wr_section.brink")
            .arg("-o")
            .arg("section_if_wr_section.bin")
            .assert()
            .success();
        let bytevec = fs::read("section_if_wr_section.bin").unwrap();
        assert_eq!(bytevec, vec![0xDEu8, 0xAD, 0xBE, 0xEF]);
        fs::remove_file("section_if_wr_section.bin").unwrap();
    }

    /// Corner case 3: compound arithmetic expression in the condition.
    /// A + B > 10 (7 + 5 = 12 > 10 → true) → emits 0x01.
    /// Verifies eval_ast_condition walks the full expression tree.
    #[test]
    fn section_if_compound_cond() {
        let _cmd = Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/section_if_compound_cond.brink")
            .arg("-o")
            .arg("section_if_compound_cond.bin")
            .assert()
            .success();
        let bytevec = fs::read("section_if_compound_cond.bin").unwrap();
        assert_eq!(bytevec, vec![0x01u8]);
        fs::remove_file("section_if_compound_cond.bin").unwrap();
    }

    // ── Top-level if/else: section definitions ───────────────────────────────

    /// Top-level if condition true: section defined inside if is promoted and
    /// referenced via a section-body if.  Expected output: 0xAA 0xBB.
    #[test]
    fn toplevel_if_section_true() {
        let _cmd = Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/toplevel_if_section_true.brink")
            .arg("-o")
            .arg("toplevel_if_section_true.bin")
            .assert()
            .success();
        let bytevec = fs::read("toplevel_if_section_true.bin").unwrap();
        assert_eq!(bytevec, vec![0xAAu8, 0xBB]);
        fs::remove_file("toplevel_if_section_true.bin").unwrap();
    }

    /// Top-level if condition false: section discarded, body if also not taken.
    /// Expected output: 0xAA only.
    #[test]
    fn toplevel_if_section_false() {
        let _cmd = Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/toplevel_if_section_false.brink")
            .arg("-o")
            .arg("toplevel_if_section_false.bin")
            .assert()
            .success();
        let bytevec = fs::read("toplevel_if_section_false.bin").unwrap();
        assert_eq!(bytevec, vec![0xAAu8]);
        fs::remove_file("toplevel_if_section_false.bin").unwrap();
    }

    /// Top-level if/else with a section in each branch under the same name.
    /// PLATFORM == 1 selects the if branch (0x01); else branch discarded.
    #[test]
    fn toplevel_if_else_section() {
        let _cmd = Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/toplevel_if_else_section.brink")
            .arg("-o")
            .arg("toplevel_if_else_section.bin")
            .assert()
            .success();
        let bytevec = fs::read("toplevel_if_else_section.bin").unwrap();
        assert_eq!(bytevec, vec![0x01u8]);
        fs::remove_file("toplevel_if_else_section.bin").unwrap();
    }

    /// Corner case: nested top-level if blocks.  A=1 promotes the inner If node;
    /// B=1 then promotes section combo.  Expected output: 0xCC.
    #[test]
    fn toplevel_if_nested_if() {
        let _cmd = Command::cargo_bin("brink")
            .unwrap()
            .arg("tests/toplevel_if_nested_if.brink")
            .arg("-o")
            .arg("toplevel_if_nested_if.bin")
            .assert()
            .success();
        let bytevec = fs::read("toplevel_if_nested_if.bin").unwrap();
        assert_eq!(bytevec, vec![0xCCu8]);
        fs::remove_file("toplevel_if_nested_if.bin").unwrap();
    }

    #[test]
    fn layout_empty_sizeof() {
        assert_brink_failure("tests/layout_empty_sizeof.brink", &["[AST_40]"]);
    }

    #[test]
    fn engine_infinite_loop() {
        assert_brink_failure("tests/engine_infinite_loop.brink", &["[EXEC_62]"]);
    }

    #[test]
    fn engine_mmap_0_byte() {
        assert_brink_failure("tests/engine_mmap_0_byte.brink", &["[EXEC_47]"]);
    }

    // ── Region system (Steps 3+4 AST layer) ─────────────────────────────────

    #[test]
    fn region_valid() {
        assert_brink_success("tests/region_valid.brink", None, None);
    }

    #[test]
    fn region_ast45_unknown_prop() {
        assert_brink_failure("tests/region_ast45_unknown_prop.brink", &["[AST_45]"]);
    }

    #[test]
    fn region_ast46_dup_prop() {
        assert_brink_failure("tests/region_ast46_dup_prop.brink", &["[AST_46]"]);
    }

    #[test]
    fn region_ast47_missing_addr() {
        assert_brink_failure("tests/region_ast47_missing_addr.brink", &["[AST_47]"]);
    }

    #[test]
    fn region_ast48_conflicts_section() {
        assert_brink_failure("tests/region_ast48_conflicts_section.brink", &["[AST_48]"]);
    }

    #[test]
    fn region_ast49_in_no_name() {
        assert_brink_failure("tests/region_ast49_in_no_name.brink", &["[AST_49]"]);
    }

    #[test]
    fn region_ast56_undeclared() {
        assert_brink_failure("tests/region_ast56_undeclared.brink", &["[AST_56]"]);
    }

    #[test]
    fn region_ast57_dup_binding() {
        assert_brink_failure("tests/region_ast57_dup_binding.brink", &["[AST_57]"]);
    }

    #[test]
    fn region_ast58_no_name() {
        assert_brink_failure("tests/region_ast58_no_name.brink", &["[AST_58]"]);
    }

    #[test]
    fn region_ast59_no_brace() {
        assert_brink_failure("tests/region_ast59_no_brace.brink", &["[AST_59]"]);
    }

    #[test]
    fn region_ast60_dup_name() {
        assert_brink_failure("tests/region_ast60_dup_name.brink", &["[AST_60]"]);
    }

    #[test]
    fn region_ast61_reserved_name() {
        assert_brink_failure("tests/region_ast61_reserved_name.brink", &["[AST_61]"]);
    }

    #[test]
    fn region_ast62_missing_eq() {
        assert_brink_failure("tests/region_ast62_missing_eq.brink", &["[AST_62]"]);
    }

    #[test]
    fn region_ast63_conflicts_const() {
        assert_brink_failure("tests/region_ast63_conflicts_const.brink", &["[AST_63]"]);
    }

    #[test]
    fn region_ast64_missing_size() {
        assert_brink_failure("tests/region_ast64_missing_size.brink", &["[AST_64]"]);
    }

    #[test]
    fn region_anchor() {
        assert_brink_success("tests/region_anchor.brink", None, None);
    }

    #[test]
    fn region_if_addr() {
        assert_brink_success("tests/region_if_addr.brink", None, None);
    }

    #[test]
    fn region_exec66() {
        assert_brink_failure("tests/region_exec66.brink", &["[EXEC_66]"]);
    }

    #[test]
    fn region_exec72() {
        assert_brink_failure("tests/region_exec72.brink", &["[EXEC_72]"]);
    }

    #[test]
    fn region_exec73() {
        assert_brink_failure("tests/region_exec73.brink", &["[EXEC_73]"]);
    }
    #[test]
    fn region_nested() {
        assert_brink_success("tests/region_nested.brink", None, None);
    }
    #[test]
    fn region_nested_2() {
        assert_brink_failure("tests/region_nested_2.brink", &["[EXEC_70]"]);
    }

    #[test]
    fn region_nested_overflow() {
        assert_brink_failure("tests/region_nested_overflow.brink", &["[EXEC_73]"]);
    }
} // mod tests
