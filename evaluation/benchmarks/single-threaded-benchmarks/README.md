# Single-Threaded Benchmarks

The program in this directory runs the benchmarks discussed in the thesis in Section 12.3.2.

The program should be built and run in release mode:

```
[nix-shell:~/benchmark]$ cargo run --release
Time Verified PT Mapping: 4661 ms
Time Verified PT Unmapping: 55787 ms
Time Verified PT Unmapping (no reclaim): 4580 ms
Time NrOS Mapping: 5341 ms
Time NrOS Unmapping: 4144 ms
```
