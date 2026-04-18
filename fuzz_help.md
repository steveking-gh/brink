# Fuzz Testing in Brink

Fuzz testing is invaluable for catching problems in compilers!  This file contains some practical notes on fuzz testing with Brink.

Fuzz tests run until stopped with Ctrl-C.  In my experience, fuzz tests will
usually catch a problem in < 60 seconds or not at all.

You will generally use the fuzz tests in process/fuzz since these exercise the
entire Brink pipeline.

Here's an example of running the fuzzer.  Note that we copy the tests into the
seed directory to give the fuzzer better starting points.

    cd process cp ../tests/* fuzz/seeds cargo +nightly fuzz run fuzz_target_1
    fuzz/corpus/fuzz_target_1 ./fuzz/seeds -- -only_ascii=1
    -dict=../tests/brink_fuzz.dict -timeout=5

Cargo fuzz uses LLVM's libFuzzer internally, which provides a vast array of
runtime options.  To see the options using the nightly compiler build:

    cargo +nightly fuzz run fuzz_target_1 -- -help=1

On Linux, if the fuzzer fails with SIGSEGV, then most likely some runaway
recursion caused a stack overflow.  In this case, the fuzzer will not produce a
useful artifact or crash report.  To determine where the error occurs, you
need to build the fuzz target, then run the fuzz target in gdb or lldb.  If
a failure occurs, you can then use `bt` to see the backtrace.

    cd process
    cargo +nightly fuzz build fuzz_target_1

Now, run the fuzz target you just built using gdb or lldb.

    gdb --args fuzz/target/x86_64-unknown-linux-gnu/release/fuzz_target_1 \
        fuzz/corpus/fuzz_target_1 ./fuzz/seeds -only_ascii=1 \
        -dict=../tests/brink_fuzz.dict -timeout=5

That starts the debugger, gdb in this example.  Run the fuzz target:

    (gdb) run
    <Failure, SIGSEGV or otherwise>
    (gdb) bt
