# Verifier Latency Evaluation

This data is discussed in Section 12.1 of the thesis.

## Raw Data

The file `times.csv` contains the raw data.

For every verification unit, the file contains the following five lines:

```
impl_u::l0,PageTableContents::accepted_mapping,total,5594
impl_u::l0,PageTableContents::accepted_mapping,rust,5219
impl_u::l0,PageTableContents::accepted_mapping,vir,310
impl_u::l0,PageTableContents::accepted_mapping,air,26
impl_u::l0,PageTableContents::accepted_mapping,smt,31
```

The first field identifies the module; The second field identifies the function name; The third
field identifies the latency component (as reported by Verus) and the fourth field contains the
measured latency in milliseconds.

The numbers we discuss as the "Verus phase" in the thesis are the sum of the `vir` and `air`
components. The Z3 phase corresponds to the component `smt`.

Some entries have an `X` instead of the component. This indicates that Verus did not generate any Z3
queries for this function and therefore we omit them from the evaluation.

```
impl_u::l0,PageTableContents::mappings_disjoint,X
```

## Reproducing the results

Because Verus does not currently support granular reporting of the verification latencies, this data
was collected with the bash script `individual_verification_times` in this directory.

The script first identifies all modules and then, for each module, individually verifies each of its
functions. To check whether Verus generates any queries, i.e. runs Z3, the script first verifies the
function once with strace and checks whether a z3 process was spawned. If so, verification is rerun
three times without strace to report the average numbers.

This data collection process is suboptimal but without dedicated support for it in Verus, it is the
best we can do.
