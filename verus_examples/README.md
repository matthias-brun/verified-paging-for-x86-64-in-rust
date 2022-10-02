# Verus Examples

To verify the proofs, follow the official instructions to install Verus:
https://github.com/verus-lang/verus.

Verification was tested and works at Verus commit `8c80c19ccd46d87cd3aa756cba7b9c5da3995d2e`.

`$RUST_VERIFY` refers to the path to the Verus installation's `rust-verify.sh` script, while
`EX_PATH` refers to the path to this directory.

To verify the first example (Section 3.2.2), run the following command:

```
$RUST_VERIFY "$EX_PATH/verified_search_tree.rs"
```

Within a few seconds this should verify:

```
verification results:: verified: 9 errors: 0
```


To verify the second example (Section 3.2.3), run the following command:

```
$RUST_VERIFY "$EX_PATH/verified_search_tree_better.rs"
```

Within a few seconds this should verify:

```
verification results:: verified: 4 errors: 0
```
