# Verified Paging for x86-64 in Rust

This repository contains the artifacts discussed in my master's thesis *Verified Paging for x86-64
in Rust*.

The contents of the repository are as follows:

* The thesis itself _thesis.pdf_.
* The directory _verified-page-table_ contains the main artifact, a page table implementation formally verified
  for functional correctness on x86-64. (A few proof steps are incomplete, refer to the thesis for
  details.)
* The directory _erased-verified-page-table_ contains a version of the implementation where all
  specification and proof code was removed. Note that this is a slightly older version than the one
  in _verified-page-table_ but the relevant functions are mostly unchanged.
* The directory _evaluation/verifier_evaluation_ contains the data discussed in Section 12.1 of the
  thesis and instructions for how to reproduce the data.
* The directory _evaluation/benchmarks/nros_benchmarks_ contains the data discussed in Section
  12.3.1 of the thesis.
* The directory _evaluation/benchmarks/single-threaded-benchmarks_ contains the data discussed in
  Section 12.3.2 of the thesis and instructions for how to reproduce the data.
* The directory _verus_examples_ contains the code used to introduce Verus in Chapter 3 of the
  thesis.
