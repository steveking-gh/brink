# Changelog

## 0.1.0

- Initial release
- Syntax highlighting for all Brink lexer tokens:
  - Keywords: section, align, set_addr, pad_addr_offset, pad_sec_offset, pad_file_offset
  - Write instructions: wr, wr8–wr64, wrs, wrf, output
  - Control flow: if, else, const
  - Intrinsics: sizeof, to_u64, to_i64
  - Address functions: addr, addr_offset, sec_offset, file_offset
  - Diagnostics: assert, print
  - Built-in constants: __OUTPUT_SIZE, __OUTPUT_ADDR, __BRINK_VERSION_*
  - Namespaced extensions: std::md5, std::sha256, std::crc32c
  - Labels, numeric literals (hex, binary, decimal, negative, typed), strings, comments
- Auto-close for `{}`, `()`, `""`
- Toggle line (`//`) and block (`/* */`) comments
- Bracket-pair colorization for `{}` and `()`
