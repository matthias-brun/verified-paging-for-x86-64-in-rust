# Verified Page Table

To verify the proofs, follow the official instructions to install Verus:
https://github.com/verus-lang/verus.

Verification was tested and works at Verus commit `8c80c19ccd46d87cd3aa756cba7b9c5da3995d2e`.

`$RUST_VERIFY` refers to the path to the Verus installation's `rust-verify.sh` script, while
`VPT_PATH` refers to the path to this directory. To verify the project, run the following command:

```
$RUST_VERIFY "$VPT_PATH/main.rs" --time --arch-word-bits 64 --deprecated-enhanced-typecheck --rlimit 100
```

Regardless of how powerful the hardware is, this should result in successful verification of the
files in this directory and report the time needed for verification.

On a laptop computer with an i7-1051U and 16GiB of memory, we see the following numbers:

```
verification results:: verified: 223 errors: 0
total-time:           69328 ms
    rust-time:             5257 ms
        init-and-types:        2989 ms
        lifetime-time:         2267 ms
        compile-time:             0 ms
    vir-time:              1803 ms
        rust-to-vir:            171 ms
        verify:                1607 ms
        erase:                    5 ms
    air-time:              1510 ms
    smt-time:             60750 ms
        smt-init:              4162 ms
        smt-run:              56588 ms
```
