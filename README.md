# brink

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

Running the command 
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


