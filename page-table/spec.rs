#[allow(unused_imports)]
use builtin::*;
use builtin_macros::*;
#[macro_use]
use crate::pervasive::*;
use seq::*;
use map::*;
#[allow(unused_imports)]
use set::*;
#[allow(unused_imports)]
use crate::{seq, seq_insert_rec, map, map_insert_rec};
#[allow(unused_imports)]
use result::{*, Result::*};

pub struct MemRegion { pub base: nat, pub size: nat }

// TODO use VAddr, PAddr

#[spec]
pub fn strictly_decreasing(s: Seq<nat>) -> bool {
    forall(|i: nat, j: nat| i < j && j < s.len() >>= s.index(i) > s.index(j))
}

// page_size, next_sizes
// 2**40    , [ 2 ** 30, 2 ** 20 ]
// 2**30    , [ 2 ** 20 ]
// 2**20    , [ ]

// [es0 # es1 , es2 , es3 ] // entry_size
// [1T  # 1G  , 1M  , 1K  ] // pages mapped at this level are this size <--

// [n0  # n1  , n2  , n3  ] // number_of_entries
// [1   # 1024, 1024, 1024] 

// es1 == es0 / n1 -- n1 * es1 == es0
// es2 == es1 / n2 -- n2 * es2 == es1
// es3 == es2 / n3 -- n3 * es3 == es2

// [es0  #  es1 , es2 , es3 , es4 ] // entry_size
// [256T #  512G, 1G  , 2M  , 4K  ]
// [n0   #  n1  , n2  , n3  , n4  ] // number_of_entries
// [     #  512 , 512 , 512 , 512 ]
// [     #  9   , 9   , 9   , 9   , 12  ]

pub struct ArchLayer {
    /// Address space size mapped by a single entry at this layer
    pub entry_size: nat,
    /// Number of entries of at this layer
    pub num_entries: nat,
}

#[spec]
pub struct Arch {
    pub layers: Seq<ArchLayer>,
    // [512G, 1G  , 2M  , 4K  ] 
    // [512 , 512 , 512 , 512 ]
}

impl Arch {
    #[spec]
    pub fn inv(&self) -> bool {
        forall(|i:nat| with_triggers!([self.layers.index(i).entry_size], [self.layers.index(i).num_entries] => i < self.layers.len() >>= (
            true
            && self.layers.index(i).entry_size > 0
            && self.layers.index(i).num_entries > 0
            && ((i + 1 < self.layers.len()) >>=
                self.layers.index(i).entry_size == self.layers.index(i as int + 1).entry_size * self.layers.index(i as int + 1).num_entries))))
    }

    #[spec] pub fn contains_entry_size(&self, entry_size: nat) -> bool {
        exists(|i: nat| #[trigger] self.layers.index(i).entry_size == entry_size)
    }
}

#[proof]
fn arch_inv_test() {
    let x86 = Arch {
        layers: seq![
            ArchLayer { entry_size: 512 * 1024 * 1024 * 1024, num_entries: 512 },
            ArchLayer { entry_size: 1 * 1024 * 1024 * 1024, num_entries: 512 },
            ArchLayer { entry_size: 2 * 1024 * 1024, num_entries: 512 },
            ArchLayer { entry_size: 4 * 1024, num_entries: 512 },
        ],
    };
    assert(x86.inv());
    assert(x86.layers.index(3).entry_size == 4096);
    assert(x86.contains_entry_size(4096));
}

#[proof]
pub struct PageTableContents {
    pub map: Map<nat /* VAddr */, MemRegion>,
    pub arch: Arch,
}

#[spec]
pub fn aligned(addr: nat, size: nat) -> bool {
    addr % size == 0
}

// TODO: overlap probably shouldn't be defined in terms of MemRegion, since it's never actually
// used that way. We always check overlap of the virtual address space.
#[spec]
pub fn overlap(region1: MemRegion, region2: MemRegion) -> bool {
    if region1.base <= region2.base {
        region2.base < region1.base + region1.size
    } else {
        region1.base < region2.base + region2.size
    }
}

fn overlap_sanity_check() {
    assert(overlap(
            MemRegion { base: 0, size: 4096 },
            MemRegion { base: 0, size: 4096 }));
    assert(overlap(
            MemRegion { base: 0, size: 8192 },
            MemRegion { base: 0, size: 4096 }));
    assert(overlap(
            MemRegion { base: 0, size: 4096 },
            MemRegion { base: 0, size: 8192 }));
    assert(overlap(
            MemRegion { base: 0, size: 8192 },
            MemRegion { base: 4096, size: 4096 }));
    assert(!overlap(
            MemRegion { base: 4096, size: 8192 },
            MemRegion { base: 0, size: 4096 }));
    assert(!overlap(
            MemRegion { base: 0, size: 4096 },
            MemRegion { base: 8192, size: 16384 }));
}

impl PageTableContents {
    #[spec]
    pub fn inv(&self) -> bool {
        true
        && self.arch.inv()
        && forall(|va: nat| with_triggers!([self.map.index(va).size],[self.map.index(va).base] => self.map.dom().contains(va) >>=
                  (aligned(va, self.map.index(va).size)
                   && aligned(self.map.index(va).base, self.map.index(va).size))))
        && forall(|b1: nat, b2: nat| // TODO verus the default triggers were bad
            with_triggers!([self.map.index(b1), self.map.index(b2)],
                           [self.map.dom().contains(b1), self.map.dom().contains(b2)] =>
                           (self.map.dom().contains(b1) && self.map.dom().contains(b2)) >>= ((b1 == b2) || !overlap(
                MemRegion { base: b1, size: self.map.index(b1).size },
                MemRegion { base: b2, size: self.map.index(b2).size }
            ))))
    }

    #[spec]
    pub fn accepted_mapping(self, base: nat, frame: MemRegion) -> bool {
        true
        && aligned(base, frame.size)
        && aligned(frame.base, frame.size)
        && self.arch.contains_entry_size(frame.size)
    }

    #[spec] pub fn valid_mapping(self, base: nat, frame: MemRegion) -> bool {
        forall(|b: nat| #[auto_trigger] self.map.dom().contains(b) >>= !overlap(
                MemRegion { base: base, size: frame.size },
                MemRegion { base: b, size: self.map.index(b).size }
                ))
    }

    /// Maps the given `frame` at `base` in the address space
    #[spec] pub fn map_frame(self, base: nat, frame: MemRegion) -> Result<PageTableContents,()> {
        if self.accepted_mapping(base, frame) {
            if self.valid_mapping(base, frame) {
                Ok(PageTableContents {
                    map: self.map.insert(base, frame),
                    ..self
                })
            } else {
                Err(())
            }
        } else {
            arbitrary()
        }
    }

    // don't think this is actually necessary for anything?
    #[proof] fn map_frame_maps_valid(#[spec] self, base: nat, frame: MemRegion) {
        requires([
            self.inv(),
            self.accepted_mapping(base, frame),
            self.valid_mapping(base, frame),
        ]);
        ensures([
            self.map_frame(base, frame).is_Ok(),
        ]);
    }

    #[proof] fn map_frame_preserves_inv(#[spec] self, base: nat, frame: MemRegion) {
        requires([
            self.inv(),
            self.accepted_mapping(base, frame),
            self.map_frame(base, frame).is_Ok(),
        ]);
        ensures([
            self.map_frame(base, frame).get_Ok_0().inv(),
        ]);
    }

    // #[proof] #[verifier(non_linear)]
    // pub fn lemma_overlap_aligned_equal_size_implies_equal_base(va1: nat, va2: nat, size: nat) {
    //     requires([
    //         aligned(va1, size),
    //         aligned(va2, size),
    //         size > 0,
    //         overlap(
    //             MemRegion { base: va1, size: size },
    //             MemRegion { base: va2, size: size }),
    //     ]);
    //     ensures(va1 == va2);
    // }

    // #[proof]
    // pub fn lemma_overlap_IMP_equal_base(self, va1: nat, base: nat, size: nat) {
    //     requires([
    //              self.inv(),
    //              self.map.dom().contains(va1),
    //              aligned(base, size),
    //              size == self.map.index(va1).size,
    //              size > 0, // TODO: this should probably be self.arch.layer_sizes.contains(size), along with 0 not being a valid size in the invariant
    //              overlap(
    //                  MemRegion { base: va1, size: self.map.index(va1).size },
    //                  MemRegion { base: base, size: size }),
    //     ]);
    //     ensures(va1 == base);

    //     if va1 <= base {
    //         // assert(va1 + va1_size <= base);
    //         if va1 < base {
    //             assert(va1 < base);
    //             assert(base < va1 + size);
    //             assert(base % size == 0);
    //             assert(va1 % size == 0);
    //             // TODO: same as below
    //             assume(false);
    //             assert(va1 == base);
    //         } else { }
    //     } else {
    //         assert(base < va1);
    //         assert(va1 < base + size);
    //         assert(va1 % size == 0);
    //         assert(base % size == 0);
    //         // assert(va1 % size == va1 - base);

    //         // base    size
    //         // |-------|
    //         //     |-------|
    //         //     va1     size
    //         // TODO: need nonlinear reasoning? (isabelle sledgehammer can prove this)
    //         assume(false);
    //         assert(va1 == base);
    //     }
    // }

    // predicate (function -> bool)
    // #[spec] pub fn step_map_frame(&self /* s */, post: &PageTableContents /* s' */, base:nat, frame: MemRegion) -> bool {
    //     post == self.map_frame(base, frame)
    // }

    // TODO later /// Changes the mapping permissions of the region containing `vaddr` to `rights`.
    // TODO later fn adjust(self, vaddr: nat) -> Result<(VAddr, usize), KError>;

    /// Given a virtual address `vaddr` it returns the corresponding `PAddr`
    /// and access rights or an error in case no mapping is found.
    // #[spec] fn resolve(self, vaddr: nat) -> MemRegion {
    #[spec] fn resolve(self, vaddr: nat) -> Result<nat,()> {
        if exists(|n:nat|
                  self.map.dom().contains(n) &&
                  n <= vaddr && vaddr < n + (#[trigger] self.map.index(n)).size) {
            let n = choose(|n:nat|
                           self.map.dom().contains(n) &&
                           n <= vaddr && vaddr < n + (#[trigger] self.map.index(n)).size);
            let offset = vaddr - n;
            Ok(self.map.index(n).base + offset)
        } else {
            Err(())
        }
    }

    /// Removes the frame from the address space that contains `base`.
    #[spec] fn unmap(self, base: nat) -> PageTableContents {
        if self.map.dom().contains(base) {
            PageTableContents {
                map: self.map.remove(base),
                ..self
            }
        } else {
            arbitrary()
        }
    }

    #[proof] fn unmap_preserves_inv(self, base: nat) {
        requires([
            self.inv(),
            self.map.dom().contains(base),
        ]);
        ensures([
            self.unmap(base).inv()
        ]);
    }
}



// Second refinement layer

#[proof] #[is_variant]
pub enum NodeEntry {
    Directory(Directory),
    Page(MemRegion),
    Empty(),
}

#[proof]
pub struct Directory {
    pub entries: Seq<NodeEntry>,
    pub layer: nat,       // index into layer_sizes
    pub base_vaddr: nat,
    pub arch: Arch,
}
// 
// // Layer 0: 425 Directory ->
// // Layer 1: 47  Directory ->
// // Layer 2: 5   Page (1K)
// 
// // Layer 1: 46  Directory -> (1M)
// // Layer 2: 1024 Pages
// 
// // Layer 0: 1024 Directories (1T)
// // Layer 1: 1024 Directories (1G)
// // Layer 2: 1024 Pages

impl Directory {

    #[spec]
    pub fn well_formed(&self) -> bool {
        true
        && self.arch.inv()
        && aligned(self.base_vaddr, self.entry_size() * self.num_entries())
        && self.layer < self.arch.layers.len()
        && self.entries.len() == self.num_entries()
    }

    #[spec]
    pub fn arch_layer(&self) -> ArchLayer {
        recommends(self.well_formed());
        self.arch.layers.index(self.layer)
    }

    #[spec]
    pub fn entry_size(&self) -> nat {
        recommends(self.layer < self.arch.layers.len());
        self.arch.layers.index(self.layer).entry_size
    }

    #[spec]
    pub fn num_entries(&self) -> nat { // number of entries
        recommends(self.layer < self.arch.layers.len());
        self.arch.layers.index(self.layer).num_entries
    }

    #[spec(checked)]
    pub fn pages_match_entry_size(&self) -> bool {
        recommends(self.well_formed());
        forall(|i: nat| (i < self.entries.len() && self.entries.index(i).is_Page())
               >>= (#[trigger] self.entries.index(i)).get_Page_0().size == self.entry_size())
    }

    #[spec(checked)]
    pub fn directories_are_in_next_layer(&self) -> bool {
        recommends(self.well_formed());
        forall(|i: nat| (i < self.entries.len() && self.entries.index(i).is_Directory())
               >>= {
                    let directory = (#[trigger] self.entries.index(i)).get_Directory_0();
                    true
                    && directory.layer == self.layer + 1
                    && directory.base_vaddr == self.base_vaddr + i * self.entry_size()
                })
    }

    #[spec(checked)]
    pub fn directories_obey_invariant(&self) -> bool {
        decreases((self.arch.layers.len() - self.layer, 0));
        recommends(self.well_formed() && self.directories_are_in_next_layer() && self.directories_match_arch());

        if self.well_formed() && self.directories_are_in_next_layer() && self.directories_match_arch() {
            forall(|i: nat| (i < self.entries.len() && self.entries.index(i).is_Directory())
                   >>= (#[trigger] self.entries.index(i)).get_Directory_0().inv())
        } else {
            arbitrary()
        }
    }

    #[spec(checked)]
    pub fn directories_match_arch(&self) -> bool {
        forall(|i: nat| (i < self.entries.len() && self.entries.index(i).is_Directory())
               >>= equal((#[trigger] self.entries.index(i)).get_Directory_0().arch, self.arch))
    }

    #[spec(checked)]
    pub fn frames_aligned(&self) -> bool {
        recommends(self.well_formed());
        forall(|i: nat| i < self.entries.len() && self.entries.index(i).is_Page() >>=
                  aligned((#[trigger] self.entries.index(i)).get_Page_0().base, self.entry_size()))
    }

    #[spec(checked)]
    pub fn inv(&self) -> bool {
        decreases(self.arch.layers.len() - self.layer);

        self.well_formed()
        && true
        && self.pages_match_entry_size()
        && self.directories_are_in_next_layer()
        && self.directories_match_arch()
        && self.directories_obey_invariant()
        && self.frames_aligned()
    }

    // forall self :: self.directories_obey_invariant()

    #[spec(checked)]
    pub fn interp(self) -> PageTableContents {
        // recommends(self.inv());
        self.interp_aux(0)
    }

    #[spec(checked)]
    pub fn interp_aux(self, i: nat) -> PageTableContents {
        // TODO: Adding the recommendation causes a warning on the recursive call, which we can't
        // prevent without writing assertions.
        // recommends(self.inv());
        decreases((self.arch.layers.len() - self.layer, self.num_entries() - i));
        // decreases_by(Self::check_interp_aux);

        if self.inv() {
            if i >= self.entries.len() {
                PageTableContents {
                    map: map![],
                    arch: self.arch,
                }
            } else { // i < self.entries.len()
                let rem = self.interp_aux(i + 1).map;
                PageTableContents {
                    map: match self.entries.index(i) {
                        NodeEntry::Page(p)      => rem.insert(self.base_vaddr + i * self.entry_size(), p),
                        NodeEntry::Directory(d) => rem.union_prefer_right(d.interp_aux(0).map),
                        NodeEntry::Empty()      => rem,
                    },
                    arch: self.arch,
                }
            }
        } else {
            arbitrary()
        }
    }

    #[proof]
    fn inv_implies_interp_aux_entries_positive_entry_size(self, i: nat) {
        decreases((self.arch.layers.len() - self.layer, self.num_entries() - i));
        requires(self.inv());
        ensures([
                forall(|va: nat| #[trigger] self.interp_aux(i).map.dom().contains(va)
                       >>= self.interp_aux(i).map.index(va).size > 0),
        ]);
        assert_forall_by(|va: nat| {
            requires(self.interp_aux(i).map.dom().contains(va));
            ensures(#[trigger] self.interp_aux(i).map.index(va).size > 0);

            if i >= self.entries.len() {
            } else {
                self.inv_implies_interp_aux_entries_positive_entry_size(i+1);
                match self.entries.index(i) {
                    NodeEntry::Page(p) => {
                        let new_va = self.base_vaddr + i * self.entry_size();
                        if new_va == va {
                        } else {
                            assert(self.interp_aux(i+1).map.index(va).size > 0);
                        }
                    },
                    NodeEntry::Directory(d) => {
                        assert(self.directories_obey_invariant());
                        d.inv_implies_interp_aux_entries_positive_entry_size(0);
                        if d.interp_aux(0).map.dom().contains(va) {
                        } else {
                            assert(self.interp_aux(i+1).map.dom().contains(va));
                        }
                    },
                    NodeEntry::Empty() => {
                        assert(self.interp_aux(i+1).map.index(va).size > 0);
                    },
                };
            }
        });
    }

    #[proof]
    fn inv_implies_interp_aux_inv(self, i: nat) {
        decreases((self.arch.layers.len() - self.layer, self.num_entries() - i));
        requires(self.inv());
        ensures([
            self.interp_aux(i).inv(),
            forall(|va: nat| #[trigger] self.interp_aux(i).map.dom().contains(va) >>= va >= self.base_vaddr + i * self.entry_size()),
            forall(|va: nat| #[trigger] self.interp_aux(i).map.dom().contains(va) >>= va <  self.base_vaddr + self.num_entries() * self.entry_size()),
            forall(|va: nat| self.interp_aux(i).map.dom().contains(va)
                   >>= va + #[trigger] self.interp_aux(i).map.index(va).size <= self.base_vaddr + self.num_entries() * self.entry_size()),
        ]);

        let interp = self.interp_aux(i);

        assert(self.directories_obey_invariant());
        assert_forall_by(|i: nat| {
            requires(i < self.entries.len() && self.entries.index(i).is_Directory());
            ensures((#[trigger] self.entries.index(i)).get_Directory_0().interp_aux(0).inv());
            self.entries.index(i).get_Directory_0().inv_implies_interp_aux_inv(0);
        });
        assert_forall_by(|va: nat| {
            requires(interp.map.dom().contains(va));
            ensures(true
                && aligned(va, (#[trigger] interp.map.index(va)).size)
                && aligned(interp.map.index(va).base, interp.map.index(va).size)
            );

            if i >= self.entries.len() {
            } else {
                let j = i + 1;
                self.inv_implies_interp_aux_inv(j);
                if self.entries.index(i).is_Page() {
                    if va < self.base_vaddr + i * self.entry_size() {
                        crate::lib::mul_distributive(i, self.entry_size());
                        assert(false);
                    } else if va == self.base_vaddr + i * self.entry_size() {
                        assert(aligned(self.base_vaddr, self.entry_size() * self.num_entries())); // TODO verus bug
                        assume(aligned(self.base_vaddr, self.entry_size())); // TODO verus nonlinear
                        assume((i * self.entry_size()) % self.entry_size() == 0); // TODO verus nonlinear
                        assert(aligned(i * self.entry_size(), self.entry_size()));
                        assume(aligned(self.base_vaddr + i * self.entry_size(), self.entry_size())); // TODO verus nonlinear
                    } else {
                    }
                }
            }
        });
        assert_forall_by(|b1: nat, b2: nat| {
            requires(interp.map.dom().contains(b1) && interp.map.dom().contains(b2) && b1 != b2);
            ensures(!overlap(
                MemRegion { base: b1, size: interp.map.index(b1).size },
                MemRegion { base: b2, size: interp.map.index(b2).size }
            ));

            if i >= self.entries.len() {
            } else {
                self.inv_implies_interp_aux_inv(i + 1);
                let (c1, c2) = if b1 < b2 {
                    (b1, b2)
                } else {
                    (b2, b1)
                };
                match self.entries.index(i) {
                    NodeEntry::Page(p) => {
                        let new_va = self.base_vaddr + i * self.entry_size();
                        if c1 != new_va && c2 != new_va {
                        } else if c1 == new_va {
                            assert(c2 >= self.base_vaddr + (i + 1) * self.entry_size());
                            crate::lib::mul_distributive(i, self.entry_size());
                        } else {
                            assert(c2 == new_va);
                            assert(c1 >= self.base_vaddr + (i + 1) * self.entry_size());
                            assert(c2 == self.base_vaddr + i * self.entry_size());
                            assume(c1 >= c2); // TODO verus nonlinear
                            assert(false);
                        }
                    },
                    NodeEntry::Directory(d) => {
                        d.inv_implies_interp_aux_inv(0);
                        assert(self.entry_size() == d.entry_size() * d.num_entries());
                        crate::lib::mul_commute(d.entry_size(), d.num_entries());
                        crate::lib::mul_distributive(i, self.entry_size());

                        let i1_interp = self.interp_aux(i + 1).map;
                        let d_interp = d.interp_aux(0).map;
                        if i1_interp.dom().contains(c1) && i1_interp.dom().contains(c2) {
                            assert_by(true
                                      && !d_interp.dom().contains(c1)
                                      && !d_interp.dom().contains(c2), {
                                          if d_interp.dom().contains(c1) {
                                              assert(c1 < self.base_vaddr + i * self.entry_size() + self.entry_size());
                                              assert(c1 < self.base_vaddr + (i + 1) * self.entry_size());
                                              assert(false);
                                          } else {
                                              if d_interp.dom().contains(c2) {
                                                  assert(c2 < self.base_vaddr + i * self.entry_size() + d.num_entries() * d.entry_size());
                                                  assert(c2 < self.base_vaddr + (i + 1) * self.entry_size());
                                              }
                                          }
                                      });
                        } else if d_interp.dom().contains(c1) && d_interp.dom().contains(c2) {
                        } else if d_interp.dom().contains(c1) && i1_interp.dom().contains(c2) {
                            assert(self.base_vaddr + (i + 1) * self.entry_size() <= c2);
                            // TODO: nonlinear
                            assert(self.base_vaddr + i * self.entry_size() + self.entry_size() <= c2);
                        } else {
                            assert(c2 <  (self.base_vaddr + i * self.entry_size()) + self.entry_size());
                            // TODO: nonlinear
                            assert(c2 <  self.base_vaddr + (i + 1) * self.entry_size());
                            assert(false);
                        }
                    },
                    NodeEntry::Empty() => (),
                }
            }
        });

        // Prove the other postconditions
        assert_forall_by(|va: nat| {
            requires(self.interp_aux(i).map.dom().contains(va));
            ensures(#[auto_trigger] true
                && va >= self.base_vaddr + i * self.entry_size()
                && va + self.interp_aux(i).map.index(va).size <= self.base_vaddr + self.num_entries() * self.entry_size()
                && va < self.base_vaddr + self.num_entries() * self.entry_size());

            if i >= self.entries.len() {
            } else {
                self.inv_implies_interp_aux_inv(i + 1);
                match self.entries.index(i) {
                    NodeEntry::Page(p) => {
                        let new_va = self.base_vaddr + i * self.entry_size();
                        if va == new_va {
                            // Post2
                            assert(equal(self.interp_aux(i).map.index(va), p));
                            assert(i < self.num_entries());
                            assert(p.size == self.entry_size());
                            // TODO: nonlinear
                            assume(i * self.entry_size() + p.size == (i + 1) * self.entry_size());
                            assert(i + 1 <= self.num_entries());
                            // TODO: nonlinear
                            assume((i + 1) * self.entry_size() <= self.num_entries() * self.entry_size());
                            assert(va + self.interp_aux(i).map.index(va).size <= self.base_vaddr + self.num_entries() * self.entry_size());
                        } else {
                            // Post1
                            assert(va >= self.base_vaddr + (i + 1) * self.entry_size());
                            // TODO: nonlinear
                            assume(va >= self.base_vaddr + i * self.entry_size());
                        }
                    },
                    NodeEntry::Directory(d) => {
                        d.inv_implies_interp_aux_inv(0);
                        let i1_interp = self.interp_aux(i + 1).map;
                        let d_interp = d.interp_aux(0).map;
                        if d_interp.dom().contains(va) {
                            // TODO:
                            assert(self.entry_size() == d.entry_size() * d.num_entries());
                            crate::lib::mul_commute(d.entry_size(), d.num_entries());
                            assert(va + self.interp_aux(i).map.index(va).size <= self.base_vaddr + i * self.entry_size() + self.entry_size());
                            // TODO: nonlinear
                            assume(va + self.interp_aux(i).map.index(va).size <= self.base_vaddr + (i + 1) * self.entry_size());
                            assert(i + 1 <= self.num_entries());
                            // TODO: nonlinear
                            assume((i + 1) * self.entry_size() <= self.num_entries() * self.entry_size());
                        } else {
                            // Post1
                            assert(va >= self.base_vaddr + (i + 1) * self.entry_size());
                            // TODO: nonlinear
                            assume(va >= self.base_vaddr + i * self.entry_size());
                        }
                    },
                    NodeEntry::Empty() => {
                        // Post1
                        assert(va >= self.base_vaddr + (i + 1) * self.entry_size());
                        // TODO: nonlinear
                        assume(va >= self.base_vaddr + i * self.entry_size());
                    },
                }
                // Post3
                self.inv_implies_interp_aux_entries_positive_entry_size(i);
            }
        });
    }

    #[proof]
    fn inv_implies_interp_inv(self) {
        requires(self.inv());
        ensures([
            self.interp().inv(),
            forall(|va: nat| #[trigger] self.interp().map.dom().contains(va) >>= va >= self.base_vaddr),
            forall(|va: nat| #[trigger] self.interp().map.dom().contains(va) >>= va <  self.base_vaddr + self.num_entries() * self.entry_size()),
            forall(|va: nat| self.interp().map.dom().contains(va)
                   >>= va + #[trigger] self.interp().map.index(va).size <= self.base_vaddr + self.num_entries() * self.entry_size()),
        ]);
        self.inv_implies_interp_aux_inv(0);
    }

    #[proof]
    fn lemma_interp_aux_disjoint(self, i: nat) {
        decreases((self.arch.layers.len() - self.layer, self.num_entries() - i));
        requires([
                 self.inv(),
                 i < self.entries.len(),
        ]);
        ensures([
                #[trigger] self.entries.index(i).is_Directory()
                >>= equal(self.interp_aux(i+1).map.union_prefer_right(self.entries.index(i).get_Directory_0().interp_aux(0).map),
                          self.entries.index(i).get_Directory_0().interp_aux(0).map.union_prefer_right(self.interp_aux(i+1).map)),
                forall(|va: nat| #[trigger] self.interp_aux(i+1).map.dom().contains(va) >>= va > self.base_vaddr + i * self.entry_size()),
        ]);

        // Post1
        if self.entries.index(i).is_Directory() {
            let rem = self.interp_aux(i+1).map;
            let d = self.entries.index(i).get_Directory_0();
            let d_interp = d.interp_aux(0).map;
            assert_forall_by(|va: nat| {
                ensures(!rem.dom().contains(va) || !d_interp.dom().contains(va));

                if rem.dom().contains(va) && d_interp.dom().contains(va) {
                    self.inv_implies_interp_aux_inv(i+1);
                    assert(va >= self.base_vaddr + (i+1) * self.entry_size());
                    assume(va > self.base_vaddr + i * self.entry_size());

                    assert(self.directories_obey_invariant());
                    d.inv_implies_interp_aux_inv(0);
                    assert(va < d.base_vaddr + d.num_entries() * d.entry_size());
                    assert(d.entry_size() * d.num_entries() == self.entry_size());
                    crate::lib::mul_commute(d.entry_size(), d.num_entries());
                    assert(va < self.base_vaddr + i * self.entry_size() + self.entry_size());
                    crate::lib::mul_distributive(i, self.entry_size());
                    assert(va < self.base_vaddr + (i+1) * self.entry_size());
                }
            });
            let un1 = rem.union_prefer_right(d_interp);
            let un2 = d_interp.union_prefer_right(rem);
            assert(un1.ext_equal(un2));
        }

        // Post2
        self.inv_implies_interp_aux_inv(i+1);
        assert(forall(|va: nat| #[trigger] self.interp_aux(i+1).map.dom().contains(va) >>= va >= self.base_vaddr + (i+1) * self.entry_size()));
        assert(self.entry_size() > 0);
        // TODO: nonlinear
        assume(forall(|va: nat| #[trigger] self.interp_aux(i+1).map.dom().contains(va) >>= va > self.base_vaddr + i * self.entry_size()));
    }

    #[proof]
    fn lemma_interp_aux_facts_page(self, i: nat, n: nat) {
        decreases((self.arch.layers.len() - self.layer, self.num_entries() - i));
        requires([
                 self.inv(),
                 i <= n,
                 n < self.entries.len(),
                 self.entries.index(n).is_Page()
        ]);
        ensures(self.interp_aux(i).map.contains_pair(self.base_vaddr + n * self.entry_size(), self.entries.index(n).get_Page_0()));

        let addr = self.base_vaddr + n * self.entry_size();
        let frame = self.entries.index(n).get_Page_0();

        if i >= self.entries.len() {
        } else {
            if i == n {
            } else {
                self.lemma_interp_aux_facts_page(i + 1, n);
                self.lemma_interp_aux_disjoint(i);
            }
        }
    }

    #[proof]
    fn lemma_interp_facts_page(self, n: nat) {
        requires([
                 self.inv(),
                 n < self.entries.len(),
                 self.entries.index(n).is_Page()
        ]);
        ensures(self.interp().map.contains_pair(self.base_vaddr + n * self.entry_size(), self.entries.index(n).get_Page_0()));
        self.lemma_interp_aux_facts_page(0, n);
    }

    #[proof]
    fn lemma_interp_aux_facts_dir(self, i: nat, n: nat, va: nat, f: MemRegion) {
        decreases((self.arch.layers.len() - self.layer, self.num_entries() - i));
        requires([
                 self.inv(),
                 i <= n,
                 n < self.entries.len(),
                 self.entries.index(n).is_Directory(),
                 self.entries.index(n).get_Directory_0().interp_aux(0).map.contains_pair(va, f),
        ]);
        ensures(self.interp_aux(i).map.contains_pair(va, f));

        if i >= self.entries.len() {
        } else { // i < self.entries.len()
            if i == n {
            } else {
                self.lemma_interp_aux_disjoint(i);
                self.lemma_interp_aux_facts_dir(i+1, n, va, f);
            }
        }
    }

    #[proof]
    fn lemma_interp_facts_dir(self, n: nat, va: nat, f: MemRegion) {
        requires([
                 self.inv(),
                 n < self.entries.len(),
                 self.entries.index(n).is_Directory(),
                 self.entries.index(n).get_Directory_0().interp().map.contains_pair(va, f),
        ]);
        ensures(self.interp().map.contains_pair(va, f));
        self.lemma_interp_aux_facts_dir(0, n, va, f);
    }

    #[proof]
    fn lemma_interp_aux_facts_empty(self, i: nat, n: nat) {
        decreases((self.arch.layers.len() - self.layer, self.num_entries() - i));
        requires([
                 self.inv(),
                 i <= n,
                 n < self.entries.len(),
                 self.entries.index(n).is_Empty(),
        ]);
        ensures(forall(|va: nat|
                       (#[trigger] self.interp_aux(i).map.dom().contains(va))
                       >>= (va + self.interp_aux(i).map.index(va).size <= self.base_vaddr + n * self.entry_size())
                       || (self.base_vaddr + (n+1) * self.entry_size() <= va)));

        assert_forall_by(|va: nat| {
            requires(#[trigger] self.interp_aux(i).map.dom().contains(va));
            ensures((va + self.interp_aux(i).map.index(va).size <= self.base_vaddr + n * self.entry_size())
                    || (self.base_vaddr + (n+1) * self.entry_size() <= va));
            if i >= self.entries.len() {
            } else {
                if i == n {
                    assert(self.interp_aux(i+1).map.dom().contains(va));
                    self.inv_implies_interp_aux_inv(i+1);
                    // assert(va >= self.base_vaddr + (i+1) * self.entry_size());
                } else {
                    self.lemma_interp_aux_facts_empty(i+1, n);
                    if va + self.interp_aux(i).map.index(va).size <= self.base_vaddr + n * self.entry_size() {
                    } else {
                        match self.entries.index(i) {
                            NodeEntry::Page(p) => {
                                if va == self.base_vaddr + i * self.entry_size() {
                                    // assert(i < n);
                                    // self.inv_implies_interp_aux_inv(i+1);
                                    // assert(forall(|va: nat| #[trigger] self.interp_aux(i+1).map.dom().contains(va) >>= va >= self.base_vaddr + (i+1) * self.entry_size()));
                                    // assert(equal(self.interp_aux(i).map.index(va), p));
                                    crate::lib::mul_distributive(i, self.entry_size());
                                    // assert(va + self.entry_size() <= self.base_vaddr + (i+1) * self.entry_size());
                                    crate::lib::mult_leq_mono1(i+1, n, self.entry_size());
                                    // assert((i+1) * self.entry_size() <= n * self.entry_size());
                                    // assert(va + self.entry_size() <= self.base_vaddr + n * self.entry_size());
                                } else {
                                    assert(self.interp_aux(i+1).map.dom().contains(va));
                                }
                            },
                            NodeEntry::Directory(d) => {
                                if !d.interp().map.dom().contains(va) {
                                    assert(self.interp_aux(i+1).map.dom().contains(va));
                                } else {
                                    assert(d.interp_aux(0).map.dom().contains(va));
                                    assert(self.directories_obey_invariant());
                                    d.inv_implies_interp_inv();
                                    assert(va + d.interp().map.index(va).size <= d.base_vaddr + d.num_entries() * d.entry_size());
                                    crate::lib::mul_distributive(i, self.entry_size());
                                    crate::lib::mult_leq_mono1(i+1, n, self.entry_size());
                                }
                            },
                            NodeEntry::Empty() => {
                                self.inv_implies_interp_aux_inv(i+1);
                            },
                        }
                    }
                }
            }
        });
    }

    #[proof]
    fn lemma_interp_facts_empty(self, n: nat) {
        requires([
                 self.inv(),
                 n < self.entries.len(),
                 self.entries.index(n).is_Empty(),
        ]);
        ensures(forall(|va: nat|
                       (#[trigger] self.interp().map.dom().contains(va))
                       >>= (va + self.interp().map.index(va).size <= self.base_vaddr + n * self.entry_size())
                       || (self.base_vaddr + (n+1) * self.entry_size() <= va)));
        self.lemma_interp_aux_facts_empty(0, n);
    }

    #[proof]
    fn lemma_interp_aux_subset_interp_aux_plus(self, i: nat, k: nat, v: MemRegion) {
        requires([
                 self.inv(),
                 self.interp_aux(i+1).map.contains_pair(k,v),
        ]);
        ensures(self.interp_aux(i).map.contains_pair(k,v));

        if i >= self.entries.len() {
        } else {
            self.lemma_interp_aux_disjoint(i);
        }
    }

    #[spec]
    fn resolve(self, vaddr: nat) -> Result<nat,()> {
        decreases(self.arch.layers.len() - self.layer);
        decreases_by(Self::check_resolve);

        if self.inv() {
            if self.base_vaddr <= vaddr && vaddr < self.base_vaddr + self.entry_size() * self.num_entries() {
                // this condition implies that "entry < self.entries.len()"
                let offset = vaddr - self.base_vaddr;
                let base_offset = offset - (offset % self.entry_size());
                let entry = base_offset / self.entry_size();
                // let _ = spec_assert(0 <= entry);
                // let _ = spec_assert(entry < self.num_entries());
                // if entry < self.entries.len() {
                match self.entries.index(entry) {
                    NodeEntry::Page(p) => {
                        Ok(p.base + offset % self.entry_size())
                    },
                    NodeEntry::Directory(d) => {
                        d.resolve(vaddr)
                    },
                    NodeEntry::Empty() => {
                        Err(())
                    },
                }
            } else {
                Err(())
            }
        } else {
            arbitrary()
        }
    }

    #[proof] #[verifier(decreases_by)]
    fn check_resolve(self, vaddr: nat) {
        if self.inv() && self.base_vaddr <= vaddr && vaddr < self.base_vaddr + self.entry_size() * self.num_entries() {
            self.resolve_prove_entry_from_if_condition(vaddr);
            assert(self.directories_obey_invariant());
        } else {
        }
    }

    // Proves 'entry < self.entries.len()', given the if condition in resolve
    #[proof]
    fn resolve_prove_entry_from_if_condition(self, vaddr: nat) {
        requires([
                 self.inv(),
                 self.base_vaddr <= vaddr,
                 vaddr < self.base_vaddr + self.entry_size() * self.num_entries(),
        ]);
        ensures({
            let offset = vaddr - self.base_vaddr;
            let base_offset = offset - (offset % self.entry_size());
            let entry = base_offset / self.entry_size();
            entry < self.entries.len()
        });
        let offset = vaddr - self.base_vaddr;
        let base_offset = offset - (offset % self.entry_size());
        let entry: nat = base_offset / self.entry_size();
        // TODO: weird nat/int cast behavior
        assume(base_offset >= 0 && self.entry_size() > 0 >>= entry >= 0);
        if self.inv() && self.base_vaddr <= vaddr && vaddr < self.base_vaddr + self.entry_size() * self.num_entries() {
            assert(offset < self.entry_size() * self.num_entries());
            crate::lib::mod_less_eq(offset, self.entry_size());
            assert(base_offset < self.entry_size() * self.num_entries());
            crate::lib::subtract_mod_aligned(offset, self.entry_size());
            // assert(aligned(base_offset, self.entry_size()));
            crate::lib::div_mul_cancel(base_offset, self.entry_size());
            assert(base_offset == base_offset / self.entry_size() * self.entry_size());
            assert(base_offset / self.entry_size() * self.entry_size() < self.entry_size() * self.num_entries());
            crate::lib::mul_commute(self.entry_size(), self.num_entries());
            crate::lib::less_mul_cancel(base_offset / self.entry_size(), self.num_entries(), self.entry_size());
            assert(base_offset / self.entry_size() < self.num_entries());
            assert(entry < self.entries.len());
        } else {
        }
    }

    #[proof]
    fn lemma_no_dir_interp_aux_mapping_implies_no_self_interp_aux_mapping(self, i: nat, n: nat, vaddr: nat, d: Directory) {
        decreases((self.arch.layers.len() - self.layer, self.num_entries() - i));
        requires([
                 self.inv(),
                 i <= n,
                 n < self.num_entries(),
                 self.entries.index(n).is_Directory(),
                 equal(d, self.entries.index(n).get_Directory_0()),
                 d.base_vaddr <= vaddr,
                 vaddr < d.base_vaddr + d.num_entries() * d.entry_size(),
                 forall(|va: nat|
                         #[trigger] d.interp().map.dom().contains(va) >>=
                         (vaddr < va || vaddr >= va + d.interp().map.index(va).size))
        ]);
        ensures(forall(|va: nat|
                       #[trigger] self.interp_aux(i).map.dom().contains(va) >>=
                       (vaddr < va || vaddr >= va + self.interp_aux(i).map.index(va).size)));

        if i >= self.entries.len() {
        } else {
            if i == n {
                assert_forall_by(|va: nat| {
                    requires(self.interp_aux(i).map.dom().contains(va));
                    ensures(vaddr < va || vaddr >= va + #[trigger] self.interp_aux(i).map.index(va).size);

                    self.inv_implies_interp_aux_inv(i+1);
                    assert(self.directories_obey_invariant());
                    d.inv_implies_interp_inv();

                    if d.interp().map.dom().contains(va) {
                    } else {
                        assert(self.interp_aux(i+1).map.dom().contains(va));

                        assert(vaddr < d.base_vaddr + d.num_entries() * d.entry_size());
                        assert(vaddr < self.base_vaddr + i * self.entry_size() + d.num_entries() * d.entry_size());
                        crate::lib::mul_commute(d.entry_size(), d.num_entries());
                        assert(vaddr < self.base_vaddr + i * self.entry_size() + self.entry_size());
                        crate::lib::mul_distributive(i, self.entry_size());
                        assert(vaddr < self.base_vaddr + (i+1) * self.entry_size());
                        assert(vaddr < va);
                    }

                });
            } else {
                self.lemma_no_dir_interp_aux_mapping_implies_no_self_interp_aux_mapping(i+1, n, vaddr, d);
                match self.entries.index(i) {
                    NodeEntry::Page(p)      => {
                        assert_forall_by(|va: nat| {
                            requires(self.interp_aux(i).map.dom().contains(va));
                            ensures(vaddr < va || vaddr >= va + #[trigger] self.interp_aux(i).map.index(va).size);

                            if self.base_vaddr + i * self.entry_size() == va {
                                assert(equal(self.interp_aux(i).map.index(va), p));
                                assert(p.size == self.entry_size());

                                assert(d.base_vaddr <= vaddr);
                                assert(self.base_vaddr + n * self.entry_size() <= vaddr);
                                assert(n >= i + 1);
                                crate::lib::mult_leq_mono1(i+1, n, self.entry_size());
                                assert(self.base_vaddr + (i+1) * self.entry_size() <= vaddr);
                                crate::lib::mul_distributive(i, self.entry_size());
                                assert(self.base_vaddr + i * self.entry_size() + self.entry_size() <= vaddr);
                                assert(self.base_vaddr + i * self.entry_size() + p.size <= vaddr);
                                assert(va + p.size <= vaddr);
                            } else {
                                assert(self.interp_aux(i+1).map.dom().contains(va));
                            }
                        });
                    },
                    NodeEntry::Directory(d2) => {
                        assert(self.directories_obey_invariant());
                        d2.inv_implies_interp_inv();
                        // assert(forall(|va: nat| #[trigger] d2.interp().map.dom().contains(va) >>= va <  d2.base_vaddr + d2.num_entries() * d2.entry_size()));
                        assert_forall_by(|va: nat| {
                            requires(self.interp_aux(i).map.dom().contains(va));
                            ensures(vaddr < va || vaddr >= va + #[trigger] self.interp_aux(i).map.index(va).size);

                            if d2.interp().map.dom().contains(va) {
                                assert(va + d2.interp().map.index(va).size <= d2.base_vaddr + d2.num_entries() * d2.entry_size());
                                assert(d.base_vaddr <= vaddr);
                                assert(self.base_vaddr + n * self.entry_size() <= vaddr);
                                assert(n >= i + 1);
                                crate::lib::mult_leq_mono1(i+1, n, self.entry_size());
                                assert(self.base_vaddr + (i+1) * self.entry_size() <= vaddr);
                                crate::lib::mul_distributive(i, self.entry_size());
                                assert(self.base_vaddr + i * self.entry_size() + self.entry_size() <= vaddr);
                                assert(d2.base_vaddr + self.entry_size() <= vaddr);
                                crate::lib::mul_commute(d2.entry_size(), d2.num_entries());
                                assert(d2.base_vaddr + d2.num_entries() * d2.entry_size() <= vaddr);
                            } else {
                                assert(self.interp_aux(i+1).map.dom().contains(va));
                            }
                        });
                    },
                    NodeEntry::Empty()      => {
                        assert(equal(self.interp_aux(i), self.interp_aux(i+1)));
                    },
                }
            }
        }
    }

    // This lemma is designed to be used with the negated abstract resolve condition, i.e.:
    // assert(!exists(|n:nat| d.interp().map.dom().contains(n) && n <= vaddr && vaddr < n + (#[trigger] d.interp().map.index(n)).size));
    // The forall version in this lemma is just easier to work with. Taking d as an argument is also done to simplify the preconditions.
    #[proof]
    fn lemma_no_dir_interp_mapping_implies_no_self_interp_mapping(self, n: nat, vaddr: nat, d: Directory) {
        requires([
                 self.inv(),
                 n < self.num_entries(),
                 self.entries.index(n).is_Directory(),
                 equal(d, self.entries.index(n).get_Directory_0()),
                 d.base_vaddr <= vaddr,
                 vaddr < d.base_vaddr + d.num_entries() * d.entry_size(),
                 forall(|va: nat|
                         #[trigger] d.interp().map.dom().contains(va) >>=
                         (vaddr < va || vaddr >= va + d.interp().map.index(va).size))
        ]);
        ensures(forall(|va: nat|
                       #[trigger] self.interp().map.dom().contains(va) >>=
                       (vaddr < va || vaddr >= va + self.interp().map.index(va).size)));

        assert(equal(self.entries.index(n).get_Directory_0().interp(), self.entries.index(n).get_Directory_0().interp_aux(0)));

        self.lemma_no_dir_interp_aux_mapping_implies_no_self_interp_aux_mapping(0, n, vaddr, d);
    }

    #[proof]
    fn resolve_refines(self, vaddr: nat) {
        decreases(self.arch.layers.len() - self.layer);
        requires([
                 self.inv(),
        ]);
        ensures([
                equal(self.interp().resolve(vaddr), self.resolve(vaddr))
        ]);

        // self.inv_implies_interp_aux_inv(0);

        if self.base_vaddr <= vaddr && vaddr < self.base_vaddr + self.entry_size() * self.num_entries() {
            let offset = vaddr - self.base_vaddr;
            let base_offset = offset - (offset % self.entry_size());
            let entry = base_offset / self.entry_size();
            self.resolve_prove_entry_from_if_condition(vaddr);
            // assume(entry < self.entries.len());
            crate::lib::subtract_mod_aligned(offset, self.entry_size());
            // assert(aligned(base_offset, self.entry_size()));
            crate::lib::div_mul_cancel(base_offset, self.entry_size());
            // assert(base_offset == base_offset / self.entry_size() * self.entry_size());
            // assert(va_base == self.base_vaddr + base_offset);
            crate::lib::mod_less_eq(offset, self.entry_size());
            // assert(offset % self.entry_size() <= offset);
            // assert(va_base == self.base_vaddr + offset - (offset % self.entry_size()));
            // assert(va_base == self.base_vaddr + (vaddr - self.base_vaddr) - ((vaddr - self.base_vaddr) % self.entry_size()));
            // assert(va_base == vaddr - ((vaddr - self.base_vaddr) % self.entry_size()));
            match self.entries.index(entry) {
                NodeEntry::Page(p) => {
                    let va_base = self.base_vaddr + entry * self.entry_size();
                    let va_base_offset = vaddr - va_base;

                    self.lemma_interp_facts_page(entry);
                    assert(self.interp().map.contains_pair(va_base, p));
                    assert(va_base <= vaddr);
                    assert(vaddr < va_base + p.size);
                    // assert(self.interp().map.dom().contains(va_base) && va_base <= vaddr && vaddr < va_base + self.interp().map.index(va_base).size);
                    assert_forall_by(|va: nat| {
                        requires(true
                                 && self.interp().map.dom().contains(va)
                                 && va <= vaddr
                                 && vaddr < va + (#[trigger] self.interp().map.index(va)).size
                                 );
                        ensures(va == va_base);

                        // assert(self.interp().map.contains_pair(va_base, p));
                        // assert(overlap(
                        //     MemRegion { base: va,      size: self.interp().map.index(va).size },
                        //     MemRegion { base: va_base, size: p.size }));
                        self.inv_implies_interp_aux_inv(0);
                    });
                    assert(equal(self.interp().resolve(vaddr), Ok(p.base + va_base_offset)));


                    assert(vaddr - va_base == offset % self.entry_size());
                    assert(equal(self.resolve(vaddr), Ok(p.base + va_base_offset)));
                },
                NodeEntry::Directory(d) => {
                    assert(self.directories_obey_invariant());
                    d.resolve_refines(vaddr);
                    assert(equal(d.interp().resolve(vaddr), d.resolve(vaddr)));

                    if d.resolve(vaddr).is_Ok() {
                        assert(self.resolve(vaddr).is_Ok());
                        assert(exists(|n: nat|
                                        d.interp().map.dom().contains(n) &&
                                        n <= vaddr && vaddr < n + (#[trigger] d.interp().map.index(n)).size));

                        let n1 = choose(|n:nat|
                                        self.interp().map.dom().contains(n) &&
                                        n <= vaddr && vaddr < n + (#[trigger] self.interp().map.index(n)).size);
                        let n2 = choose(|n:nat|
                                        d.interp().map.dom().contains(n) &&
                                        n <= vaddr && vaddr < n + (#[trigger] d.interp().map.index(n)).size);

                        assert(self.entries.index(entry).get_Directory_0().interp().map.contains_pair(n2, d.interp().map.index(n2)));
                        self.lemma_interp_facts_dir(entry, n2, d.interp().map.index(n2));

                        assert_forall_by(|n1: nat, n2: nat| {
                            requires(
                                self.interp().map.dom().contains(n1) &&
                                n1 <= vaddr && vaddr < n1 + (#[trigger] self.interp().map.index(n1)).size &&
                                self.interp().map.dom().contains(n2) &&
                                n2 <= vaddr && vaddr < n2 + (#[trigger] self.interp().map.index(n2)).size);
                            ensures(n1 == n2);
                            self.inv_implies_interp_inv();
                            assert(self.interp().inv());
                        });

                        assert(n1 == n2);
                        let n = n1;
                        assert(self.interp().map.dom().contains(n));
                        assert(d.resolve(vaddr).is_Ok());
                        assert(d.interp().resolve(vaddr).is_Ok());
                        assert(equal(d.interp().resolve(vaddr), self.interp().resolve(vaddr)));
                    } else {
                        assert(d.resolve(vaddr).is_Err());
                        assert(self.resolve(vaddr).is_Err());
                        // assert(self.interp().resolve(vaddr).is_Err());
                        // assume(!exists(|n:nat|
                        //                d.interp().map.dom().contains(n) &&
                        //                n <= vaddr && vaddr < n + (#[trigger] d.interp().map.index(n)).size));
                        if self.interp().resolve(vaddr).is_Ok() {
                            assert(exists(|n:nat|
                                          self.interp().map.dom().contains(n) &&
                                          n <= vaddr && vaddr < n + (#[trigger] self.interp().map.index(n)).size));
                            let n = choose(|n:nat|
                                           self.interp().map.dom().contains(n) &&
                                           n <= vaddr && vaddr < n + (#[trigger] self.interp().map.index(n)).size);
                            assert(self.interp().map.dom().contains(n));
                            // self
                            // |---------------------|
                            //           |-d-| |-d2|
                            //           |-n-|
                            assert(n <= vaddr);
                            assert(vaddr < n + self.interp().map.index(n).size);

                            // assume(d.base_vaddr <= n);
                            // assume(n < d.base_vaddr + d.num_entries() * d.entry_size());

                            // !exists(|va: nat| d.interp().map.dom().contains(va) && d.interp().map.index(n) = { _,size } && va <= n && n < va + size && d.base_vaddr <= va && va < d.base_vaddr + d.num_entries() * d.entry_size())
                            // !exists(|va: nat| self.interp().map.dom().contains(va) && self.interp().map.index(n) = { _,size } && va <= n && n < va + size)
                            // assert(!exists(|va: nat|
                            //         d.interp().map.dom().contains(va)
                            //         && va <= vaddr
                            //         && vaddr < va + d.interp().map.index(va).size));
                            // assert(d.base_vaddr <= vaddr);

                            // let offset = vaddr - self.base_vaddr;
                            // let base_offset = offset - (offset % self.entry_size());
                            // let entry = base_offset / self.entry_size();

                            assert(self.entry_size() > 0);
                            assume(offset % self.entry_size() < self.entry_size());
                            assume(vaddr < vaddr + self.entry_size() - (offset % self.entry_size()));
                            assume(vaddr < self.base_vaddr - self.base_vaddr + vaddr + self.entry_size() - (offset % self.entry_size()));
                            assume(vaddr < self.base_vaddr + vaddr - self.base_vaddr + self.entry_size() - (offset % self.entry_size()));
                            assume(vaddr < self.base_vaddr + (vaddr - self.base_vaddr) + self.entry_size() - (offset % self.entry_size()));
                            assume(vaddr < self.base_vaddr + ((vaddr - self.base_vaddr) + self.entry_size() - (offset % self.entry_size())));
                            assume(vaddr < self.base_vaddr + ((offset - (offset % self.entry_size())) + self.entry_size()));
                            assume(vaddr < self.base_vaddr + (base_offset + self.entry_size()));
                            assume(vaddr < self.base_vaddr + ((base_offset + self.entry_size()) / self.entry_size()) * self.entry_size());
                            assume(vaddr < self.base_vaddr + ((base_offset / self.entry_size())+1) * self.entry_size());
                            assume(vaddr < self.base_vaddr + (entry+1) * self.entry_size());
                            assume(vaddr < (self.base_vaddr + entry * self.entry_size()) + self.entry_size());
                            assume(vaddr < (self.base_vaddr + entry * self.entry_size()) + d.num_entries() * d.entry_size());
                            assume(vaddr < d.base_vaddr + d.num_entries() * d.entry_size());


                            self.lemma_no_dir_interp_mapping_implies_no_self_interp_mapping(entry, vaddr, d);
                            // assert(!exists(|va: nat|
                            //                d.interp().map.dom().contains(va)
                            //                && va <= vaddr
                            //                && vaddr < va + d.interp().map.index(va).size
                            //                && d.base_vaddr <= va
                            //                && va < d.base_vaddr + d.num_entries() * d.entry_size()));
                            // assume(!exists(|va: nat| self.interp().map.dom().contains(va) && va <= vaddr && vaddr < va + self.interp().map.index(va).size));
                            assert(self.interp().map.dom().contains(n) && n <= vaddr && vaddr < n + self.interp().map.index(n).size);

                            // assert(d.interp().resolve(vaddr).is_Err());

                            // assume(false);
                        }
                        assert(self.interp().resolve(vaddr).is_Err());
                        assert(d.interp().resolve(vaddr).is_Err());
                        assert(equal(d.interp().resolve(vaddr), self.interp().resolve(vaddr)));
                    }
                    assert(equal(d.interp().resolve(vaddr), self.interp().resolve(vaddr)));

                },
                NodeEntry::Empty() => {
                    assert(self.resolve(vaddr).is_Err());

                    assert_forall_by(|n: nat| {
                        requires(#[trigger] self.interp().map.dom().contains(n) && n <= vaddr && vaddr < n + self.interp().map.index(n).size);
                        ensures(false);

                        self.lemma_interp_facts_empty(entry);
                        assert((n + self.interp().map.index(n).size <= self.base_vaddr + entry * self.entry_size())
                               || (self.base_vaddr + (entry+1) * self.entry_size() <= n));
                        if n + self.interp().map.index(n).size <= self.base_vaddr + entry * self.entry_size() {
                        } else {
                            self.inv_implies_interp_inv();
                            // assert(n + self.interp().map.index(n).size > self.base_vaddr + entry * self.entry_size());
                            assert(self.base_vaddr + (entry+1) * self.entry_size() <= n);
                            assert(self.base_vaddr + (entry+1) * self.entry_size() <= vaddr);
                            assert_by(false, {
                                // let offset = vaddr - self.base_vaddr;
                                // let base_offset = offset - (offset % self.entry_size());
                                // let entry = base_offset / self.entry_size();

                                // TODO: nonlinear
                                assume(offset % self.entry_size() < self.entry_size());
                                assume(vaddr < vaddr + self.entry_size() - (offset % self.entry_size()));
                                assume(vaddr < self.base_vaddr - self.base_vaddr + vaddr + self.entry_size() - (offset % self.entry_size()));
                                assume(vaddr < self.base_vaddr + vaddr - self.base_vaddr + self.entry_size() - (offset % self.entry_size()));
                                assume(vaddr < self.base_vaddr + (vaddr - self.base_vaddr) + self.entry_size() - (offset % self.entry_size()));
                                assume(vaddr < self.base_vaddr + ((vaddr - self.base_vaddr) + self.entry_size() - (offset % self.entry_size())));
                                assume(vaddr < self.base_vaddr + ((offset - (offset % self.entry_size())) + self.entry_size()));
                                assume(vaddr < self.base_vaddr + (base_offset + self.entry_size()));
                                assume(vaddr < self.base_vaddr + ((base_offset + self.entry_size()) / self.entry_size()) * self.entry_size());
                                assume(vaddr < self.base_vaddr + ((base_offset / self.entry_size())+1) * self.entry_size());
                                assume(vaddr < self.base_vaddr + (entry+1) * self.entry_size());
                            });
                        }
                    });
                    assert(self.interp().resolve(vaddr).is_Err());
                },
            }
        } else {
            assert(self.resolve(vaddr).is_Err());

            self.inv_implies_interp_inv();
            if vaddr >= self.base_vaddr + self.entry_size() * self.num_entries() {
                assert(forall(|va: nat| self.interp().map.dom().contains(va)
                              >>= va + #[trigger] self.interp().map.index(va).size <= self.base_vaddr + self.num_entries() * self.entry_size()));
                assert(self.base_vaddr <= vaddr);
                if self.interp().resolve(vaddr).is_Ok() {
                    assert(exists(|n: nat| #[trigger] self.interp().map.dom().contains(n) && n <= vaddr && vaddr < n + self.interp().map.index(n).size));
                    let va = choose(|n: nat| #[trigger] self.interp().map.dom().contains(n) && n <= vaddr && vaddr < n + self.interp().map.index(n).size);
                    crate::lib::mul_commute(self.entry_size(), self.num_entries());
                    assert(va + self.interp().map.index(va).size <= self.base_vaddr + self.num_entries() * self.entry_size());
                    assert(false);
                }
            } else {
            }
            assert(self.interp().resolve(vaddr).is_Err());
        }
    }

    // #[proof]
    // fn resolve_aux_properties(self, vaddr: nat) {
    //     decreases(self.arch.layers.len() - self.layer);
    //     requires([
    //              self.inv(),
    //              self.resolve(vaddr).is_Ok()
    //     ]);
    //     // ensures(self.resolve(vaddr).is_Ok() >>= self.interp().resolve(vaddr).is_Ok());
    //     ensures(exists(|n:nat| self.interp().map.dom().contains(n) && n <= vaddr && vaddr < n + self.interp().map.index(n).size));

    //     // assume(false);
    //     if self.base_vaddr <= vaddr && vaddr < self.base_vaddr + self.entry_size() * self.num_entries() {
    //         let offset = vaddr - self.base_vaddr;
    //         let base_offset = offset - (offset % self.entry_size());
    //         let entry: nat = base_offset / self.entry_size();
    //         assume(base_offset >= 0 && self.entry_size() > 0 >>= entry >= 0);
    //         // FIXME: proved this in the check function already; really tedious proof
    //         assume(entry < self.entries.len());
    //         assert(self.entry_size() > 0);
    //         match self.entries.index(entry) {
    //             NodeEntry::Page(p) => {
    //                 // let n = self.base_vaddr + p.base + offset % self.entry_size();
    //                 let n = self.base_vaddr + entry * self.entry_size();
    //                 assert(n == self.base_vaddr + base_offset / self.entry_size() * self.entry_size());
    //                 assume(aligned(base_offset, self.entry_size()));
    //                 crate::lib::div_mul_cancel(base_offset, self.entry_size());
    //                 assert(base_offset == base_offset / self.entry_size() * self.entry_size());
    //                 assert(n == self.base_vaddr + base_offset);
    //                 crate::lib::mod_less_eq(offset, self.entry_size());
    //                 assert(offset % self.entry_size() <= offset);
    //                 assert(n == self.base_vaddr + offset - (offset % self.entry_size()));
    //                 assert(n == self.base_vaddr + (vaddr - self.base_vaddr) - ((vaddr - self.base_vaddr) % self.entry_size()));
    //                 assert(n == vaddr - ((vaddr - self.base_vaddr) % self.entry_size()));
    //                 assert(self.base_vaddr <= vaddr);
    //                 // FIXME: need an interp lemma
    //                 assume(self.interp().map.dom().contains(n));
    //                 // assert(self.interp().map.index(n).size == self.entry_size());
    //                 assert(n <= vaddr);
    //                 assert(vaddr < n + self.entry_size());
    //                 // FIXME: need an interp lemma
    //                 assume(vaddr < n + self.interp().map.index(n).size);
    //             },
    //             NodeEntry::Directory(d) => {
    //                 assert(self.directories_obey_invariant());
    //                 d.resolve_aux_properties(vaddr);
    //                 let k = choose(|n:nat| d.interp().map.dom().contains(n) && n <= vaddr && vaddr < n + d.interp().map.index(n).size);
    //                 assert(d.interp().map.dom().contains(k) && k <= vaddr && vaddr < k + d.interp().map.index(k).size);
    //                 // FIXME
    //                 assume(forall(|n:nat,k:nat,v:MemRegion|
    //                               (true
    //                               && n < self.num_entries()
    //                               && self.entries.index(n).is_Directory()
    //                               && self.entries.index(n).get_Directory_0().interp().map.contains_pair(k,v))
    //                               >>= self.interp().map.contains_pair(k,v)));
    //                 let v = d.interp().map.index(k);
    //                 assert(d.interp().map.contains_pair(k,v));
    //                 assert(self.interp().map.dom().contains(k) && k <= vaddr && vaddr < k + self.interp().map.index(k).size);
    //             },
    //             NodeEntry::Empty() => { },
    //         }
    //     } else {
    //     }
    // }

    // #[proof]
    // fn am_i_crazy(self, i: int, j: nat) {
    //     requires(i == -3 && j == 3);
    //     assert(i as nat >= 0);
    //     let entry: nat = i as nat / j;
    //     // let entry: nat = (arbitrary::<nat>() as int) as nat;
    //     assert(entry >= 0);
    // }

}

#[proof]
pub fn lemma_set_contains_IMP_len_greater_zero<T>(s: Set<T>, a: T) {
    requires([
             s.finite(),
             s.contains(a)
    ]);
    ensures(s.len() > 0);

    if s.len() == 0 {
        // contradiction
        assert(s.remove(a).len() + 1 == 0);
    }
}

#[spec]
fn spec_assert(p: bool) {
    recommends(p);
    ()
}
