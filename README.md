# Brink

__WARNING: Brink is under construction.  Please come back later!__

__^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^__



Brink is a domain specific language for linking and compositing of
an output file.  Brink simplifies construction of complex files by managing sizes,
offsets and ordering in a readable declarative style.  Brink was created with
flash and other NVM images in mind, but tries to be generally useful.

## Examples

For a source file called my_file.brink:

    /*
     * A section defines part of an output.
     */
    section foo {
        // wrs writes a quoted string to the output
        wrs "Hello World!\n";
    }

    // An output statement outputs the section
    output foo;

Running the command:
`brink my_file.brink` Produces a file containing the string `Hello World!\n`.

Brink supports assert expressions for error checking.  This example verifies that the size of the section is 13 bytes long.

    section bar {
        wrs "Hello World!\n";
        assert sizeof(bar) == 13;
    }
    output bar;

## Unit Testing

Brink supports unit tests.

    cargo test

## Fuzz Testing

Brink supports fuzz tests for its various submodules.  Fuzz testing starts from
a corpus of random inputs and then further randomizes those inputs to try to
cause crashes and hangs.  At the time of writing (Rust 1.49.0), fuzz testing
**required the nightly build**.

To run fuzz tests:

    cd process
    cargo +nightly fuzz run fuzz_target_1

    cd lineardb
    cargo +nightly fuzz run fuzz_target_1

    cd ast
    cargo +nightly fuzz run fuzz_target_1

Fuzz tests run until stopped with Ctrl-C.  In my experience, fuzz tests will catch a problem in 60 seconds or not at all.

Cargo fuzz uses LLVM's libFuzzer internally, which provides a vast array of runtime options.  To see thh options using the nightly compiler build:

    cargo +nightly fuzz run fuzz_target_1 -- -help=1

A copy of this help output is in the fuzz_help.txt file.

For example, setting a smaller 5 second timeout for hangs and maximum input length of 256 bytes.

    cargo +nightly fuzz run fuzz_target_1 -- -timeout=5 -max_len=256

## Basic Structure of a Brink Program

A brink source file consists of one or more section definitions and exactly one output statement.    Each section has a unique name.  The output statement specifies the name of the top level section.  Starting from the top section, Brink recursively evaluates each section and produces the output file.  For example, we can define a section with a write-string (wrs) expression:

    section foo {
        wrs "I'm foo";
    }

    output foo;

Produces a default output named `output.bin`.

    $ cat output.bin
    I'm foo



Using a write (wr) statement, sections can write other sections:

    section foo {
        wrs "I'm foo\n";
    }

    section bar {
        wrs "I'm bar\n";
        wr foo;
    }

    output bar;

Produces `output.bin`:

    $ cat output.bin
    I'm bar
    I'm foo

---
## Brink Language Reference
---

## Literals

### Numbers

Brink supports number literals in decimal, hex (0x) and binary (0b) forms.  After the first digit, you can use '_' within number literals to help with readability.  Brink uses the [parse_int](https://crates.io/crates/parse_int) library for conversion from string to value.

    assert 42 == 42;
    assert 0x42 == 0x42;
    assert 0x42 == 66;
    assert 0x4_2 == 66;
    assert 0x42 == 6_6;

    assert 0b0 == 0;
    assert 0b01000010 == 0x42;
    assert 0b0100_0010 == 0x42;
    assert 0b101000010 == 0x142;
    assert 0b0000000001000010 == 0x42;

Numbers are 64-bit unsigned (u64) by default.

### Quoted Strings

Brink allows utf-8 quoted strings with escape characters tab (\t) and newline (\n).  Newlines are Linux style, so "A\n" is a two byte string on all platforms.

## Arithmetic Operators

Brink supports the following arithmetic operators with same relative precedence as the Rust language.

| Precedence | Operator | Under/Overflow Check? | Description                                   |
|------------|----------|-----------------------|-----------------------------------------------|
| Highest    | (   )    | n/a                   | Paren grouping                                |
|            | *   /    | yes                   | Multiply and divide                           |
|            | +   -    | yes                   | Add and subtract                              |
|            | &        | n/a                   | Bitwise-AND                                   |
|            | \|       | n/a                   | Bitwise-OR                                    |
|            | <<  >>   | no                    | Bitwise shift up and down                     |
|            | ==  !=   | n/a                   | Equals and non-equal                          |
|            | =>       | n/a                   | Greater-than-or-equal (same precedence as ==) |
|            | <=       | n/a                   | less-than-or-equalLTE (same precedence as ==) |
|            | &&       | n/a                   | Logical-AND                                   |
| Lowest     | \|\|     | n/a                   | Logical-OR                                    |

As shown in the table, Brink will check some operations for arithmetic under/overflow.

## Address and Offset

The following built-in functions allows programs to query and use three kinds of location information:
* `abs( [optional identifier] )` returns an absolute address
* `img( [optional identifier] )` returns an offset relative to the start of the output
* `sec( [optional identifier] )` returns an offset relative to the start of a section

When called with an identifier, the address or offset pertains to the location of that identifier.  When called without an identifier, the functions return the current address or offset.

For example:

    section fiz {
        assert abs() == 6;
        assert img() == 6;
        assert sec() == 0;
        wrs "fiz";
        assert abs() == 9;
        assert img() == 9;
        assert sec() == 3;
    }
    
    section bar {
        assert abs() == 3;
        assert img() == 3;
        assert sec() == 0;
        wrs "bar";
        assert abs() == 6;
        assert img() == 6;
        assert sec() == 3;
        wr fiz;
        assert abs() == 9;
        assert img() == 9;
        assert sec() == 6;
    }
    
    // top level section
    section foo {
        assert abs() == 0;
        assert img() == 0;
        assert sec() == 0;
        wrs "foo";
        assert abs() == 3;
        assert img() == 3;
        assert sec() == 3;
        wr bar;
        assert abs() == 9;
        assert img() == 9;
        assert sec() == 9;
    }
    
    output foo;