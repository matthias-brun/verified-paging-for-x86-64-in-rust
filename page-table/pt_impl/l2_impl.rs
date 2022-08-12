#![allow(unused_imports)]
use builtin::*;
use builtin_macros::*;
use crate::pervasive::*;
use modes::*;
use seq::*;
use option::{*, Option::*};
use map::*;
use set::*;
use set_lib::*;
use vec::*;
use crate::lib_axiom::*;

use crate::lib::aligned;
use result::{*, Result::*};

use crate::pt_impl::l1;
use crate::pt_impl::l0::{ArchExec,Arch,MemRegion,MemRegionExec,ambient_arith};
use crate::pt_impl::l0::{MAX_BASE,MAX_NUM_ENTRIES,MAX_NUM_LAYERS,MAX_ENTRY_SIZE};

verus! {

// FIXME: We can probably remove bits from here that we don't use, e.g. accessed, dirty, PAT. (And
// set them to zero when we create a new entry.)
#[is_variant]
pub ghost enum GhostPageDirectoryEntry {
    Directory {
        addr: usize,
        /// Present; must be 1 to map a page or reference a directory
        flag_P: bool,
        /// Read/write; if 0, writes may not be allowed to the page controlled by this entry
        flag_RW: bool,
        /// User/supervisor; user-mode accesses are not allowed to the page controlled by this entry
        flag_US: bool,
        /// Page-level write-through
        flag_PWT: bool,
        /// Page-level cache disable
        flag_PCD: bool,
        /// Accessed; indicates whether software has accessed the page referenced by this entry
        flag_A: bool,
        /// If IA32_EFER.NXE = 1, execute-disable (if 1, instruction fetches are not allowed from
        /// the page controlled by this entry); otherwise, reserved (must be 0)
        flag_XD: bool,
    },
    Page {
        addr: usize,
        /// Present; must be 1 to map a page or reference a directory
        flag_P: bool,
        /// Read/write; if 0, writes may not be allowed to the page controlled by this entry
        flag_RW: bool,
        /// User/supervisor; user-mode accesses are not allowed to the page controlled by this entry
        flag_US: bool,
        /// Page-level write-through
        flag_PWT: bool,
        /// Page-level cache disable
        flag_PCD: bool,
        /// Accessed; indicates whether software has accessed the page referenced by this entry
        flag_A: bool,
        /// Dirty; indicates whether software has written to the page referenced by this entry
        flag_D: bool,
        // /// Page size; must be 1 (otherwise, this entry references a directory)
        // flag_PS: Option<bool>,
        // PS is entirely determined by the Page variant and the layer
        /// Global; if CR4.PGE = 1, determines whether the translation is global; ignored otherwise
        flag_G: bool,
        /// Indirectly determines the memory type used to access the page referenced by this entry
        flag_PAT: bool,
        /// If IA32_EFER.NXE = 1, execute-disable (if 1, instruction fetches are not allowed from
        /// the page controlled by this entry); otherwise, reserved (must be 0)
        flag_XD: bool,
    },
    Empty,
}

const MAXPHYADDR: u64 = 52;

macro_rules! bit {
    ($v:expr) => {
        1u64 << $v
    }
}
// Generate bitmask where bits $low:$high are set to 1. (inclusive on both ends)
macro_rules! bitmask_inc {
    ($low:expr,$high:expr) => {
        (!(!0u64 << (($high+1u64)-$low))) << $low
    }
}
// macro_rules! bitmask {
//     ($low:expr,$high:expr) => {
//         (!(!0 << ($high-$low))) << $low
//     }
// }

// FIXME: I messed up the layers, deeper layers are bigger numbers but I wrote predicates
// assuming 0 is the lowest layer
// layer:
// 0 -> PML4
// 1 -> PDPT, Page Directory Pointer Table
// 2 -> PD, Page Directory
// 3 -> PT, Page Table


// MASK_FLAG_* are flags valid for all entries.
const MASK_FLAG_P:    u64 = bit!(0u64);
const MASK_FLAG_RW:   u64 = bit!(1u64);
const MASK_FLAG_US:   u64 = bit!(2u64);
const MASK_FLAG_PWT:  u64 = bit!(3u64);
const MASK_FLAG_PCD:  u64 = bit!(4u64);
const MASK_FLAG_A:    u64 = bit!(5u64);
const MASK_FLAG_XD:   u64 = bit!(63u64);
// We can use the same address mask for all layers as long as we preserve the invariant that the
// lower bits that *should* be masked off are already zero.
const MASK_ADDR:      u64 = bitmask_inc!(12u64,MAXPHYADDR);
// const MASK_ADDR:      u64 = 0b0000000000001111111111111111111111111111111111111111000000000000;

// MASK_PG_FLAG_* are flags valid for all page mapping entries, unless a specialized version for that
// layer exists, e.g. for layer 3 MASK_L3_PG_FLAG_PAT is used rather than MASK_PG_FLAG_PAT.
const MASK_PG_FLAG_D:    u64 = bit!(6u64);
const MASK_PG_FLAG_G:    u64 = bit!(8u64);
const MASK_PG_FLAG_PAT:  u64 = bit!(12u64);

const MASK_L1_PG_FLAG_PS:   u64 = bit!(7u64);
const MASK_L2_PG_FLAG_PS:   u64 = bit!(7u64);

const MASK_L3_PG_FLAG_PAT:  u64 = bit!(7u64);

// const MASK_DIR_REFC:           u64 = bitmask_inc!(52u64,62u64); // Ignored bits for storing refcount in L3 and L2
// const MASK_DIR_L1_REFC:        u64 = bitmask_inc!(8u64,12u64); // Ignored bits for storing refcount in L1
// const MASK_DIR_REFC_SHIFT:     u64 = 52u64;
// const MASK_DIR_L1_REFC_SHIFT:  u64 = 8u64;
const MASK_DIR_ADDR:           u64 = MASK_ADDR;

// We should be able to always use the 12:52 mask and have the invariant state that in the
// other cases, the lower bits are already zero anyway.
const MASK_L1_PG_ADDR:      u64 = bitmask_inc!(30u64,MAXPHYADDR);
const MASK_L2_PG_ADDR:      u64 = bitmask_inc!(21u64,MAXPHYADDR);
const MASK_L3_PG_ADDR:      u64 = bitmask_inc!(12u64,MAXPHYADDR);

proof fn lemma_addr_masks_facts(address: u64)
    ensures
        MASK_L2_PG_ADDR & address == address ==> MASK_L3_PG_ADDR & address == address,
        MASK_L1_PG_ADDR & address == address ==> MASK_L3_PG_ADDR & address == address,
{
    // TODO: can we get support for consts in bit vector reasoning?
    assert((bitmask_inc!(21u64, 52u64) & address == address) ==> (bitmask_inc!(12u64, 52u64) & address == address)) by (bit_vector);
    assert((bitmask_inc!(30u64, 52u64) & address == address) ==> (bitmask_inc!(12u64, 52u64) & address == address)) by (bit_vector);
}

proof fn lemma_addr_masks_facts2(address: u64)
    ensures
        (address & MASK_L3_PG_ADDR) & MASK_L2_PG_ADDR == address & MASK_L2_PG_ADDR,
        (address & MASK_L3_PG_ADDR) & MASK_L1_PG_ADDR == address & MASK_L1_PG_ADDR,
{
    assert(((address & bitmask_inc!(12u64, 52u64)) & bitmask_inc!(21u64, 52u64)) == (address & bitmask_inc!(21u64, 52u64))) by (bit_vector);
    assert(((address & bitmask_inc!(12u64, 52u64)) & bitmask_inc!(30u64, 52u64)) == (address & bitmask_inc!(30u64, 52u64))) by (bit_vector);
}

// // MASK_PD_* are flags valid for all entries pointing to another directory
// const MASK_PD_ADDR:      u64 = bitmask!(12,52);

pub open spec fn addr_is_zero_padded(layer: nat, addr: u64, is_page: bool) -> bool {
    is_page ==> {
        if layer == 1 {
            addr & MASK_ADDR == addr & MASK_L1_PG_ADDR
        } else if layer == 2 {
            addr & MASK_ADDR == addr & MASK_L2_PG_ADDR
        } else if layer == 3 {
            addr & MASK_ADDR == addr & MASK_L3_PG_ADDR
        } else {
            true
        }
    }
}


// An entry in any page directory (i.e. in PML4, PDPT, PD or PT)
#[repr(transparent)]
pub struct PageDirectoryEntry {
    pub entry: u64,
    // pub view: Ghost<GhostPageDirectoryEntry>,
    pub ghost layer: nat,
}

impl PageDirectoryEntry {

    pub open spec fn view(self) -> GhostPageDirectoryEntry {
        if self.layer() <= 3 {
            let v = self.entry;
            if v & MASK_FLAG_P == MASK_FLAG_P {
                let addr     = (v & MASK_ADDR) as usize;
                let flag_P   = v & MASK_FLAG_P   == MASK_FLAG_P;
                let flag_RW  = v & MASK_FLAG_RW  == MASK_FLAG_RW;
                let flag_US  = v & MASK_FLAG_US  == MASK_FLAG_US;
                let flag_PWT = v & MASK_FLAG_PWT == MASK_FLAG_PWT;
                let flag_PCD = v & MASK_FLAG_PCD == MASK_FLAG_PCD;
                let flag_A   = v & MASK_FLAG_A   == MASK_FLAG_A;
                let flag_XD  = v & MASK_FLAG_XD  == MASK_FLAG_XD;
                if (self.layer() == 3) || (v & MASK_L1_PG_FLAG_PS == MASK_L1_PG_FLAG_PS) {
                    let flag_D   = v & MASK_PG_FLAG_D   == MASK_PG_FLAG_D;
                    let flag_G   = v & MASK_PG_FLAG_G   == MASK_PG_FLAG_G;
                    let flag_PAT = if self.layer() == 3 { v & MASK_PG_FLAG_PAT == MASK_PG_FLAG_PAT } else { v & MASK_L3_PG_FLAG_PAT == MASK_L3_PG_FLAG_PAT };
                    GhostPageDirectoryEntry::Page {
                        addr,
                        flag_P, flag_RW, flag_US, flag_PWT, flag_PCD,
                        flag_A, flag_D, flag_G, flag_PAT, flag_XD,
                    }
                } else {
                    GhostPageDirectoryEntry::Directory {
                        addr, flag_P, flag_RW, flag_US, flag_PWT, flag_PCD, flag_A, flag_XD,
                    }
                }
            } else {
                GhostPageDirectoryEntry::Empty
            }
        } else {
            arbitrary()
        }
    }

    pub open spec fn addr_is_zero_padded(self) -> bool {
        addr_is_zero_padded(self.layer, self.entry, self@.is_Page())
    }

    pub open spec fn inv(self) -> bool {
        &&& self.layer() <= 3
        &&& self.addr_is_zero_padded()
    }

    pub open spec fn layer(self) -> nat {
        self.layer
    }

    pub proof fn lemma_new_entry_addr_mask_is_address(
        layer: usize,
        address: u64,
        is_page: bool,
        is_writable: bool,
        is_supervisor: bool,
        is_writethrough: bool,
        disable_cache: bool,
        disable_execute: bool,
        )
        requires
            layer <= 3,
            if is_page { 0 < layer } else { layer < 3 },
            addr_is_zero_padded(layer, address, is_page),
        ensures
            ({ let e = address
                | MASK_FLAG_P
                | if is_page && layer != 3 { MASK_L1_PG_FLAG_PS }  else { 0 }
                | if is_writable           { MASK_FLAG_RW }        else { 0 }
                | if is_supervisor         { MASK_FLAG_US }        else { 0 }
                | if is_writethrough       { MASK_FLAG_PWT }       else { 0 }
                | if disable_cache         { MASK_FLAG_PCD }       else { 0 }
                | if disable_execute       { MASK_FLAG_XD }        else { 0 };
                e & MASK_ADDR == address
            }),
    {
        assume(false);
    }

    pub fn new_page_entry(layer: usize, address: u64) -> (r: Self)
        requires
            0 < layer <= 3,
            addr_is_zero_padded(layer, address, true),
        ensures
            r.inv(),
            r@.is_Page(),
            // r@.get_Page_addr() == address,
    {
        // FIXME: check what flags we want here
        Self::new_entry(layer, address, true, true, true, false, false, false)
    }

    pub fn new_dir_entry(layer: usize, address: u64) -> (r: Self)
        requires
            layer < 3,
            address & MASK_DIR_ADDR == address
        ensures
            r.inv(),
            r@.is_Directory(),
            // r@.get_Directory_addr() == address,
    {
        // FIXME: check what flags we want here
        Self::new_entry(layer, address, false, true, true, false, false, false)
    }

    pub fn new_entry(
        layer: usize,
        address: u64,
        is_page: bool,
        is_writable: bool,
        is_supervisor: bool, // TODO: is this inverted, 0 is user-mode-access allowed, 1 is disallowed
        is_writethrough: bool,
        disable_cache: bool,
        disable_execute: bool,
        ) -> (r: PageDirectoryEntry)
        requires
            layer <= 3,
            if is_page { 0 < layer } else { layer < 3 },
            addr_is_zero_padded(layer, address, is_page),
        ensures
            if is_page { r@.is_Page() } else { r@.is_Directory() },
            r.inv(),
    {
        let e =
        PageDirectoryEntry {
            entry: {
                address
                | MASK_FLAG_P
                | if is_page && layer != 3 { MASK_L1_PG_FLAG_PS }  else { 0 }
                | if is_writable           { MASK_FLAG_RW }        else { 0 }
                | if is_supervisor         { MASK_FLAG_US }        else { 0 }
                | if is_writethrough       { MASK_FLAG_PWT }       else { 0 }
                | if disable_cache         { MASK_FLAG_PCD }       else { 0 }
                | if disable_execute       { MASK_FLAG_XD }        else { 0 }
            },
            layer: layer as nat,
        };

        proof {
            assert(e.layer() <= 3);
            if e.layer() <= 3 {
                if e.entry & MASK_FLAG_P == MASK_FLAG_P {
                    if e.layer() == 3 {
                        assert(is_page);
                        assert(e@.is_Page());
                    } else if e.entry & MASK_L1_PG_FLAG_PS == MASK_L1_PG_FLAG_PS {
                        // FIXME: bitvector
                        assume(is_page);
                        assert(e@.is_Page());
                    } else {
                        // FIXME: bitvector
                        assume(!is_page);
                        assert(e@.is_Directory());
                    }
                } else {
                    // FIXME: bitvector
                    assume(false);
                }
            }
            assert(if is_page { e@.is_Page() } else { e@.is_Directory() });

            if is_page {
                assert_by(e.addr_is_zero_padded(), {
                    // lemma_addr_masks_facts(address);
                    // lemma_addr_masks_facts2(e.entry);
                    // Self::lemma_new_entry_addr_mask_is_address(layer, address, is_page, is_writable, is_supervisor, is_writethrough, disable_cache, disable_execute);
                    // assert(addr_is_zero_padded(layer, address, true));
                    // FIXME: bitvector
                    // Need to show that we aren't setting any of the bits that are masked off by
                    // the L1/L2 masks but not masked off by MASK_ADDR
                    if e.layer() == 1 {
                        assume(e.entry & MASK_ADDR == e.entry & MASK_L1_PG_ADDR);
                    } else if e.layer() == 2 {
                        assume(e.entry & MASK_ADDR == e.entry & MASK_L2_PG_ADDR);
                    } else if e.layer() == 3 {
                        assert(e.entry & MASK_ADDR == e.entry & MASK_L3_PG_ADDR);
                    }
                });
            } else {
                assert(e.addr_is_zero_padded());
            }
        }
        e
    }

    pub fn address(&self) -> (res: u64)
        requires
            self.layer() <= 3,
            !self@.is_Empty(),
        ensures
            res as usize == match self@ {
                GhostPageDirectoryEntry::Page { addr, .. }      => addr,
                GhostPageDirectoryEntry::Directory { addr, .. } => addr,
                GhostPageDirectoryEntry::Empty                  => arbitrary(),
            }
    {
        self.entry & MASK_ADDR
    }

    pub fn is_mapping(&self) -> (r: bool)
        requires
            self.layer() <= 3
        ensures
            r == !self@.is_Empty(),
    {
        (self.entry & MASK_FLAG_P) == MASK_FLAG_P
    }

    pub fn is_page(&self, layer: usize) -> (r: bool)
        requires
            !self@.is_Empty(),
            layer as nat == self.layer,
            layer <= 3,
        ensures
            if r { self@.is_Page() } else { self@.is_Directory() },
    {
        (layer == 3) || ((self.entry & MASK_L1_PG_FLAG_PS) == MASK_L1_PG_FLAG_PS)
    }

    pub fn is_dir(&self, layer: usize) -> (r: bool)
        requires
            !self@.is_Empty(),
            layer as nat == self.layer,
            layer <= 3,
        ensures
            if r { self@.is_Directory() } else { self@.is_Page() },
    {
        !self.is_page(layer)
    }
}


// FIXME: We need to allow the dirty and accessed bits to change in the memory.
// Or maybe we just specify reads to return those bits as arbitrary?
#[verifier(external_body)]
pub struct PageTableMemory {
    // how is the memory range for this represented?
    ptr: *mut u8,
}

impl PageTableMemory {
    pub open spec fn root(&self) -> usize { arbitrary() }

    #[verifier(external_body)]
    fn root_exec(&self) -> (res: usize)
        ensures
            res == self.root()
    {
        unreached()
    }

    pub open spec fn view(&self) -> Seq<nat> { arbitrary() }

    // We assume that alloc_page never fails. In practice we can just keep a buffer of 3+ pages
    // that are allocated before we use map_frame.
    /// Allocates one page and returns a pointer to it as the offset from self.root()
    #[verifier(external_body)]
    fn alloc_page(&self) -> (res: usize)
        // ensures
        //     res + 4096 <= self@.len(),
            // FIXME: reconsider the view for the memory, maybe it should be a struct with spec
            // read and write for u64 instead
            // mixed trigger
            // forall|i: nat| i < 4096 ==> #[trigger] self@.index(res + i) == 0,
    {
        // FIXME:
        unreached()
    }

    #[verifier(external_body)]
    fn write(&mut self, ptr: usize, value: u64)
        // FIXME: reconsider view and this pre-/postcondition
        // requires
        //     ptr < old(self)@.len(),
        // ensures
        //     forall|i: nat| i < self@.len() ==> self@.index(i) == value,
    {
        // FIXME:
        unreached()
        // unsafe {
        //     self.ptr.offset(ptr as isize).write(value)
        // }
    }

    // byte offset or word offset?
    #[verifier(external_body)]
    fn read(&self, offset: usize) -> (res: u64)
        // FIXME: probably need precondition here and extend the invariant
        // requires
        //     offset < self@.len(),
        ensures
            res == self.spec_read(offset)
    {
        // unsafe { std::slice::from_raw_parts(self.ptr.offset(offset as isize), ENTRY_BYTES) }
        0 // FIXME: unimplemented
    }

    pub open spec fn spec_read(self, offset: nat) -> (res: u64);
}

pub struct PageTable {
    pub memory: PageTableMemory,
    pub arch: ArchExec,
}

const ENTRY_BYTES: usize = 8;

impl PageTable {


    pub open spec(checked) fn well_formed(self, layer: nat) -> bool {
        &&& self.arch@.inv()
    }

    pub open spec(checked) fn inv(&self) -> bool {
        self.inv_at(0, self.memory.root(), 0)
    }

    /// Get the view of the entry at address ptr + i * ENTRY_BYTES
    pub open spec fn view_at(self, layer: nat, ptr: usize, i: nat) -> GhostPageDirectoryEntry {
        PageDirectoryEntry {
            entry: self.memory.spec_read(ptr as nat + i * ENTRY_BYTES),
            layer,
        }@
    }

    /// Get the entry at address ptr + i * ENTRY_BYTES
    #[verifier(nonlinear)]
    fn entry_at(&self, layer: usize, ptr: usize, i: usize) -> (res: PageDirectoryEntry)
        ensures
            res.layer == layer,
            res@ === self.view_at(layer, ptr, i),
    {
        // FIXME:
        assume(ptr <= 100);
        assume(i * ENTRY_BYTES <= 100000);
        PageDirectoryEntry {
            entry: self.memory.read(ptr + i * ENTRY_BYTES),
            layer,
        }
    }

    // pub open spec fn directories_obey_invariant_at(self, layer: nat, ptr: usize) -> bool
    //     decreases (self.arch@.layers.len() - layer, 0nat)
    // {
    //     decreases_when(self.well_formed(layer) && self.layer_in_range(layer));
    //     forall|i: nat| i < self.arch@.num_entries(layer) ==> {
    //         let entry = #[trigger] self.view_at(layer, ptr, i);
    //         // #[trigger] PageDirectoryEntry { entry: u64_from_le_bytes(self.get_entry_bytes(ptr, i)), layer: Ghost::new(layer) }@;
    //         entry.is_Directory() ==> self.inv_at(layer + 1, entry.get_Directory_addr())
    //     }
    // }

    pub open spec fn empty_at(self, layer: nat, ptr: usize) -> bool
        recommends self.well_formed(layer)
    {
        forall|i: nat| i < self.arch@.num_entries(layer) ==> self.view_at(layer, ptr, i).is_Empty()
    }

    // pub open spec fn directories_are_nonempty_at(self, layer: nat, ptr: usize) -> bool
    //     recommends self.well_formed(layer)
    // {
    //     forall|i: nat| i < self.arch@.num_entries(layer) ==> {
    //         let entry = #[trigger] self.view_at(layer, ptr, i);
    //         entry.is_Directory() ==> !self.empty_at(layer + 1, entry.get_Directory_addr())
    //     }
    // }

    // pub open spec(checked) fn frames_aligned(self, layer: nat, ptr: usize) -> bool
    //     recommends self.well_formed(layer) && self.layer_in_range(layer)
    // {
    //     forall|i: nat| i < self.arch@.num_entries(layer) ==> {
    //         let entry = #[trigger] self.view_at(layer, ptr, i);
    //         entry.is_Page() ==> aligned(entry.get_Page_addr(), self.arch@.entry_size(layer))
    //     }
    // }

    pub open spec(checked) fn layer_in_range(self, layer: nat) -> bool {
        layer < self.arch@.layers.len()
    }

    pub open spec(checked) fn layer_depth_is_finite(self, layer: nat, ptr: usize) -> bool
        decreases (self.arch@.layers.len() - layer, 1nat)
    {
        &&& self.well_formed(layer)
        &&& self.layer_in_range(layer)
        &&& self.directories_layer_depth_is_finite(layer, ptr)
    }

    pub open spec fn directories_layer_depth_is_finite(self, layer: nat, ptr: usize) -> bool
        decreases (self.arch@.layers.len() - layer, 0nat)
    {
        decreases_when(self.well_formed(layer) && self.layer_in_range(layer));
        forall|i: nat| i < self.arch@.num_entries(layer) ==> {
            let entry = #[trigger] self.view_at(layer, ptr, i);
            entry.is_Directory() ==> self.layer_depth_is_finite(layer + 1, entry.get_Directory_addr())
        }
    }

    pub open spec fn directories_obey_invariant_at(self, layer: nat, ptr: usize, base_vaddr: nat) -> bool
        decreases (self.arch@.layers.len() - layer, 0nat)
    {
        decreases_when(self.well_formed(layer) && self.layer_in_range(layer));
        forall|i: nat| i < self.arch@.num_entries(layer) ==> {
            let entry = #[trigger] self.view_at(layer, ptr, i);
            // #[trigger] PageDirectoryEntry { entry: u64_from_le_bytes(self.get_entry_bytes(ptr, i)), layer: Ghost::new(layer) }@;
            entry.is_Directory() ==> self.inv_at(layer + 1, entry.get_Directory_addr(), self.arch@.entry_base(layer, base_vaddr, i))
        }
    }

    pub open spec(checked) fn inv_at(&self, layer: nat, ptr: usize, base_vaddr: nat) -> bool
        decreases self.arch@.layers.len() - layer
    {
        // Some redundancy here but it's probably for the best.
        &&& self.layer_depth_is_finite(layer, ptr)
        &&& self.interp_at(layer, ptr, base_vaddr).inv()
        &&& self.directories_obey_invariant_at(layer, ptr, base_vaddr)
        // &&& self.directories_are_nonempty_at(layer, ptr)
        // &&& self.frames_aligned(layer, ptr)
    }

    pub open spec fn interp_at(self, layer: nat, ptr: usize, base_vaddr: nat) -> l1::Directory
        decreases (self.arch@.layers.len() - layer, self.arch@.num_entries(layer), 1nat)
    {
        decreases_when(self.layer_depth_is_finite(layer, ptr));
        l1::Directory {
            entries: self.interp_at_aux(layer, ptr, base_vaddr, seq![]),
            layer: layer,
            base_vaddr,
            arch: self.arch@,
        }
    }

    pub open spec fn interp_at_aux(self, layer: nat, ptr: usize, base_vaddr: nat, init: Seq<l1::NodeEntry>) -> Seq<l1::NodeEntry>
        decreases (self.arch@.layers.len() - layer, self.arch@.num_entries(layer) - init.len(), 0nat)
    {
        decreases_when(self.layer_depth_is_finite(layer, ptr));
        decreases_by(Self::termination_interp_at_aux);
        if init.len() >= self.arch@.num_entries(layer) {
            init
        } else {
            let entry = match self.view_at(layer, ptr, init.len()) {
                GhostPageDirectoryEntry::Directory { addr: dir_addr, .. } => {
                    let new_base_vaddr = self.arch@.entry_base(layer, base_vaddr, init.len());
                    l1::NodeEntry::Directory(self.interp_at(layer + 1, dir_addr, new_base_vaddr))
                },
                GhostPageDirectoryEntry::Page { addr, .. } =>
                    l1::NodeEntry::Page(MemRegion { base: addr, size: self.arch@.entry_size(layer) }),
                GhostPageDirectoryEntry::Empty =>
                    l1::NodeEntry::Empty(),
            };
            self.interp_at_aux(layer, ptr, base_vaddr, init.add(seq![entry]))
        }
    }

    #[proof] #[verifier(decreases_by)]
    spec fn termination_interp_at_aux(self, layer: nat, ptr: usize, base_vaddr: nat, init: Seq<l1::NodeEntry>) {
        // assert(self.directories_obey_invariant_at(layer, ptr));
        assert(self.arch@.layers.len() - (layer + 1) < self.arch@.layers.len() - layer);
        // FIXME: why isn't this going through?
        // Can I somehow assert the decreases here or assert an inequality between tuples?
        assume(false);
    }

    spec fn interp(self) -> l1::Directory {
        self.interp_at(0, self.memory.root(), 0)
    }

    proof fn lemma_inv_implies_interp_inv(self)
        requires
            self.inv(),
        ensures self.interp().inv()
    {
        crate::lib::aligned_zero();
        assert(forall_arith(|a: nat, b: nat| a > 0 && b > 0 ==> #[trigger] (a * b) > 0)) by(nonlinear_arith);
        assert(self.arch@.entry_size(0) * self.arch@.num_entries(0) > 0);
        assert(aligned(0, self.arch@.entry_size(0) * self.arch@.num_entries(0)));
        self.lemma_inv_at_implies_interp_at_inv(0, self.memory.root(), 0);
    }

    proof fn lemma_inv_at_implies_interp_at_inv(self, layer: nat, ptr: usize, base_vaddr: nat)
        requires
            self.inv_at(layer, ptr, base_vaddr),
            aligned(base_vaddr, self.arch@.entry_size(layer) * self.arch@.num_entries(layer)),
        ensures
            self.interp_at(layer, ptr, base_vaddr).inv(),
            self.interp_at(layer, ptr, base_vaddr).interp().inv(),
            self.interp_at(layer, ptr, base_vaddr).interp().upper == self.arch@.upper_vaddr(layer, base_vaddr),
            self.interp_at(layer, ptr, base_vaddr).interp().lower == base_vaddr,
            !self.empty_at(layer, ptr) ==> !self.interp_at(layer, ptr, base_vaddr).empty(),
            ({ let res = self.interp_at(layer, ptr, base_vaddr);
                forall|j: nat|
                    #![trigger res.entries.index(j)]
                    j < res.entries.len() ==>
                    match self.view_at(layer, ptr, j) {
                        GhostPageDirectoryEntry::Directory { addr: dir_addr, .. }  => {
                            &&& res.entries.index(j).is_Directory()
                            &&& res.entries.index(j).get_Directory_0() === self.interp_at((layer + 1) as nat, dir_addr, self.arch@.entry_base(layer, base_vaddr, j))
                        },
                        GhostPageDirectoryEntry::Page { addr, .. }             => res.entries.index(j).is_Page() && res.entries.index(j).get_Page_0().base == addr,
                        GhostPageDirectoryEntry::Empty                         => res.entries.index(j).is_Empty(),
                    }
            }),
        decreases (self.arch@.layers.len() - layer, self.arch@.num_entries(layer), 1nat)
    {
        self.lemma_inv_at_implies_interp_at_aux_inv(layer, ptr, base_vaddr, seq![]);
        let res = self.interp_at(layer, ptr, base_vaddr);
        assert(res.pages_match_entry_size());
        assert(res.directories_are_in_next_layer());
        assert(res.directories_match_arch());
        assert(res.directories_obey_invariant());
        assert(res.directories_are_nonempty());
        assert(res.frames_aligned());
        res.lemma_inv_implies_interp_inv();
    }

    proof fn lemma_inv_at_implies_interp_at_aux_inv(self, layer: nat, ptr: usize, base_vaddr: nat, init: Seq<l1::NodeEntry>)
        requires
            self.inv_at(layer, ptr, base_vaddr),
            aligned(base_vaddr, self.arch@.entry_size(layer) * self.arch@.num_entries(layer)),
        ensures
            self.interp_at_aux(layer, ptr, base_vaddr, init).len() == if init.len() > self.arch@.num_entries(layer) { init.len() } else { self.arch@.num_entries(layer) },
            forall|j: nat| j < init.len() ==> #[trigger] self.interp_at_aux(layer, ptr, base_vaddr, init).index(j) === init.index(j),
            ({ let res = self.interp_at_aux(layer, ptr, base_vaddr, init);
                forall|j: nat|
                    init.len() <= j && j < res.len() && res.index(j).is_Directory()
                    ==> {
                        let dir = res.index(j).get_Directory_0();
                        // directories_obey_invariant
                        &&& dir.inv()
                        // directories_are_in_next_layer
                        &&& dir.layer == layer + 1
                        &&& dir.base_vaddr == base_vaddr + j * self.arch@.entry_size(layer)
                        // directories_match_arch@
                        &&& dir.arch === self.arch@
                        // directories_are_nonempty
                        &&& !dir.empty()
                        &&& self.view_at(layer, ptr, j).is_Directory()
            }}),
            ({ let res = self.interp_at_aux(layer, ptr, base_vaddr, init);
                forall|j: nat|
                    init.len() <= j && j < res.len() && res.index(j).is_Page()
                    ==> {
                        let page = res.index(j).get_Page_0();
                        // pages_match_entry_size
                        &&& page.size == self.arch@.entry_size(layer)
                        // frames_aligned
                        &&& aligned(page.base, self.arch@.entry_size(layer))
                        &&& self.view_at(layer, ptr, j).is_Page()
                        &&& self.view_at(layer, ptr, j).get_Page_addr() == page.base
                    }
            }),
            ({ let res = self.interp_at_aux(layer, ptr, base_vaddr, init);
                forall|j: nat|
                    init.len() <= j && j < res.len() && res.index(j).is_Empty()
                    ==> (#[trigger] self.view_at(layer, ptr, j)).is_Empty()
            }),
            // This could be merged with some of the above stuff by writing it as an iff instead
            ({ let res = self.interp_at_aux(layer, ptr, base_vaddr, init);
                forall|j: nat|
                    #![trigger res.index(j)]
                    init.len() <= j && j < res.len() ==>
                    match self.view_at(layer, ptr, j) {
                        GhostPageDirectoryEntry::Directory { addr: dir_addr, .. }  => {
                            &&& res.index(j).is_Directory()
                            &&& res.index(j).get_Directory_0() === self.interp_at((layer + 1) as nat, dir_addr, self.arch@.entry_base(layer, base_vaddr, j))
                        },
                        GhostPageDirectoryEntry::Page { addr, .. } => res.index(j).is_Page() && res.index(j).get_Page_0().base == addr,
                        GhostPageDirectoryEntry::Empty             => res.index(j).is_Empty(),
                    }
            }),
        decreases (self.arch@.layers.len() - layer, self.arch@.num_entries(layer) - init.len(), 0nat)
    {
        if init.len() >= self.arch@.num_entries(layer) {
        } else {
            // assert(self.directories_obey_invariant_at(layer, ptr));
            let entry = match self.view_at(layer, ptr, init.len()) {
                GhostPageDirectoryEntry::Directory { addr: dir_addr, .. } => {
                    let new_base_vaddr = self.arch@.entry_base(layer, base_vaddr, init.len());
                    l1::NodeEntry::Directory(self.interp_at(layer + 1, dir_addr, new_base_vaddr))
                },
                GhostPageDirectoryEntry::Page { addr, .. } =>
                    l1::NodeEntry::Page(MemRegion { base: addr, size: self.arch@.entry_size(layer) }),
                GhostPageDirectoryEntry::Empty =>
                    l1::NodeEntry::Empty(),
            };
            let init_next = init.add(seq![entry]);
            let res      = self.interp_at_aux(layer, ptr, base_vaddr, init);
            let res_next = self.interp_at_aux(layer, ptr, base_vaddr, init_next);

            self.lemma_inv_at_implies_interp_at_aux_inv(layer, ptr, base_vaddr, init_next);

            assert(res === res_next);
            assert(res.len() == self.arch@.num_entries(layer));
            assert(res.index(init.len()) === entry);

            assert forall|j: nat|
                init.len() <= j && j < res.len() && res.index(j).is_Directory()
                implies {
                    let dir = res.index(j).get_Directory_0();
                    // directories_obey_invariant
                    &&& dir.inv()
                    // directories_are_in_next_layer
                    &&& dir.layer == layer + 1
                    &&& dir.base_vaddr == base_vaddr + j * self.arch@.entry_size(layer)
                    // directories_match_arch@
                    &&& dir.arch === self.arch@
                    // directories_are_nonempty
                    &&& !dir.empty()
                }
            by {
                let dir = res.index(j).get_Directory_0();
                if init.len() == j {
                    match self.view_at(layer, ptr, init.len()) {
                        GhostPageDirectoryEntry::Directory { addr: dir_addr, .. } => {
                            let new_base_vaddr = self.arch@.entry_base(layer, base_vaddr, init.len());
                            assert(self.inv_at(layer + 1, dir_addr, new_base_vaddr));
                            self.arch@.lemma_entry_base();
                            assert(aligned(new_base_vaddr, self.arch@.entry_size(layer + 1) * self.arch@.num_entries(layer + 1)));
                            self.lemma_inv_at_implies_interp_at_inv(layer + 1, dir_addr, new_base_vaddr);
                            assert(dir.inv());
                            assert(dir.layer == layer + 1);
                            assert(dir.base_vaddr == base_vaddr + j * self.arch@.entry_size(layer));
                            assert(dir.arch === self.arch@);
                            // assert(self.directories_are_nonempty_at(layer, ptr));
                            assert(!self.empty_at(layer + 1, dir_addr));
                            assert(!dir.empty());
                        },
                        GhostPageDirectoryEntry::Page { addr, .. } => (),
                        GhostPageDirectoryEntry::Empty => (),
                    };
                } else {
                }
            };
        }
    }

    #[allow(unused_parens)] // https://github.com/secure-foundations/verus/issues/230
    fn resolve_aux(&self, layer: usize, ptr: usize, base: usize, vaddr: usize) -> (res: (Result<usize, ()>))
        requires
            self.inv_at(layer, ptr, base),
            self.interp_at(layer, ptr, base).interp().accepted_resolve(vaddr),
            base <= vaddr < MAX_BASE,
            aligned(base, self.arch@.entry_size(layer) * self.arch@.num_entries(layer)),
        ensures
            // Refinement of l1
            res.map_ok(|v: usize| v as nat) === self.interp_at(layer, ptr, base).resolve(vaddr),
            // Refinement of l0
            res.map_ok(|v: usize| v as nat) === self.interp_at(layer, ptr, base).interp().resolve(vaddr),
        // decreases self.arch@.layers.len() - layer
    {
        let idx: usize = self.arch.index_for_vaddr(layer, base, vaddr);
        let entry      = self.entry_at(layer, ptr, idx);
        proof {
            self.lemma_inv_at_implies_interp_at_inv(layer, ptr, base);
            self.arch@.lemma_index_for_vaddr(layer, base, vaddr);
        }
        let interp:     Ghost<l1::Directory> = ghost(self.interp_at(layer, ptr, base));
        let interp_res: Ghost<Result<nat,()>> = ghost(interp@.resolve(vaddr));
        proof {
            assert(interp_res@ === self.interp_at(layer, ptr, base).resolve(vaddr));
            interp@.lemma_resolve_refines(vaddr);
        }
        if entry.is_mapping() {
            let entry_base: usize = self.arch.entry_base(layer, base, idx);
            proof {
                self.arch@.lemma_entry_base();
                assert(entry_base <= vaddr);
            }
            if entry.is_dir(layer) {
                let dir_addr = entry.address() as usize;
                proof {
                    // assert(self.directories_obey_invariant_at(layer, ptr));
                    assert(self.inv_at((layer + 1) as nat, dir_addr, entry_base));
                    self.lemma_inv_at_implies_interp_at_inv((layer + 1) as nat, dir_addr, entry_base);
                    assert(self.interp_at((layer + 1) as nat, dir_addr, entry_base).interp().accepted_resolve(vaddr));
                }
                self.resolve_aux(layer + 1, dir_addr, entry_base, vaddr)
            } else {
                assert(entry@.is_Page());
                let offset: usize = vaddr - entry_base;
                // FIXME: need to assume a maximum for physical addresses
                assume(entry@.get_Page_addr() < 10000);
                assert(offset < self.arch@.entry_size(layer));
                Ok(entry.address() as usize + offset)
            }
        } else {
            Err(())
        }
    }

    #[allow(unused_parens)] // https://github.com/secure-foundations/verus/issues/230
    fn resolve(&self, vaddr: usize) -> (res: (Result<usize,()>))
        requires
            self.inv(),
            self.interp().interp().accepted_resolve(vaddr),
            vaddr < MAX_BASE,
        ensures
            // Refinement of l1
            res.map_ok(|v: usize| v as nat) === self.interp().resolve(vaddr),
            // Refinement of l0
            res.map_ok(|v: usize| v as nat) === self.interp().interp().resolve(vaddr),
    {
        proof { ambient_arith(); }
        self.resolve_aux(0, self.memory.root_exec(), 0, vaddr)
    }

    spec fn accepted_mapping(self, layer: nat, ptr: usize, base: nat, vaddr: nat, frame: MemRegion) -> bool {
        &&& 0 < layer // Can't map pages in PML4
        &&& self.interp_at(layer, ptr, base).accepted_mapping(vaddr, frame)
    }

    #[allow(unused_parens)] // https://github.com/secure-foundations/verus/issues/230
    fn map_frame_aux(&mut self, layer: usize, ptr: usize, base: usize, vaddr: usize, frame: MemRegionExec) -> (res: (Result<(),()>))
        requires
            old(self).inv_at(layer, ptr, base),
            old(self).accepted_mapping(layer, ptr, base, vaddr, frame@),
            base <= vaddr < MAX_BASE,
            aligned(base, old(self).arch@.entry_size(layer) * old(self).arch@.num_entries(layer)),
        ensures
            old(self).empty_at(layer, ptr) ==> res.is_Ok(),
            // self.inv_at(layer, ptr, base),
            // // Refinement of l1
            // match res {
            //     Ok(res) =>
            //         Ok(self.interp_at(layer, ptr, base)) === old(self).interp_at(layer, ptr, base).map_frame(vaddr, frame@),
            //     Err(e)  => Err(self.interp_at(layer, ptr, base)) === old(self).interp_at(layer, ptr, base).map_frame(vaddr, frame@),
            // },
        //     // Refinement of l0
        //     match res {
        //         Ok(res) =>
        //             Ok(self.interp_at(layer, ptr, base).interp())
        //                 === old(self).interp_at(layer, ptr, base).interp().map_frame(vaddr, frame@),
        //         Err(e)  => Err(e) === old(self).interp_at(layer, ptr, base).interp().map_frame(vaddr, frame@),
        //     }
        // decreases self.arch@.layers.len() - layer
    {
        let idx: usize = self.arch.index_for_vaddr(layer, base, vaddr);
        let entry      = self.entry_at(layer, ptr, idx);
        proof {
            self.lemma_inv_at_implies_interp_at_inv(layer, ptr, base);
            self.arch@.lemma_index_for_vaddr(layer, base, vaddr);
        }
        let entry_base: usize = self.arch.entry_base(layer, base, idx);
        proof {
            self.arch@.lemma_entry_base();
            assert(entry_base <= vaddr);
        }
        let res = if entry.is_mapping() {
            if entry.is_dir(layer) {
                if self.arch.entry_size(layer) == frame.size {
                    Err(())
                } else {
                    let dir_addr = entry.address() as usize;
                    proof {
                        // assert(self.directories_obey_invariant_at(layer, ptr));
                        assert(self.inv_at((layer + 1) as nat, dir_addr, entry_base));
                        self.lemma_inv_at_implies_interp_at_inv((layer + 1) as nat, dir_addr, entry_base);

                        let dir_interp_l1 = self.interp_at((layer + 1) as nat, dir_addr, entry_base);
                        self.interp_at(layer, ptr, base).lemma_accepted_mapping_implies_directory_accepted_mapping(vaddr, frame@, dir_interp_l1);
                        assert(dir_interp_l1.interp().accepted_mapping(vaddr, frame@));
                    }
                    self.map_frame_aux(layer + 1, dir_addr, entry_base, vaddr, frame)
                }
            } else {
                Err(())
            }
        } else {
            if self.arch.entry_size(layer) == frame.size {
                proof {
                    let frame_base = frame.base as u64;
                    // FIXME: this may have to be part of accepted_mapping?
                    assume(addr_is_zero_padded(layer, frame_base, true));
                }
                let new_page_entry = PageDirectoryEntry::new_page_entry(layer, frame.base as u64);
                assume(ptr < 100); assume(idx < 100);
                self.memory.write(ptr + idx * ENTRY_BYTES, new_page_entry.entry);
                Ok(())
            } else {
                let new_dir_ptr     = self.memory.alloc_page();
                let new_dir_ptr_u64 = new_dir_ptr as u64;
                // FIXME: this should be a postcondition of alloc_page
                assume(new_dir_ptr_u64 & MASK_DIR_ADDR == new_dir_ptr_u64);
                let new_dir_entry = PageDirectoryEntry::new_dir_entry(layer, new_dir_ptr_u64);
                // assume(forall|i:nat| i < 512 ==> self.memory.spec_read
                assume(ptr < 100); assume(idx < 100);
                self.memory.write(ptr + idx * ENTRY_BYTES, new_dir_entry.entry);
                assume(self.empty_at((layer + 1) as nat, new_dir_ptr));
                assume(self.inv_at((layer + 1) as nat, new_dir_ptr, entry_base));
                assume(self.accepted_mapping((layer + 1) as nat, new_dir_ptr, entry_base, vaddr, frame@));
                let ghostself: Ghost<PageTable> = ghost(*self);
                let res = self.map_frame_aux(layer + 1, new_dir_ptr, entry_base, vaddr, frame);
                assert(ghostself@.empty_at((layer + 1) as nat, new_dir_ptr) ==> res.is_Ok());
                assert(res.is_Ok());
                res
                    // self.map_frame_aux(layer + 1, new_dir_ptr, entry_base, vaddr, frame) === self.interp_at(layer + 1, ...).map_frame(vaddr, frame@)
                    // self.map_frame_aux(layer, ptr, base, vaddr, frame) === self.interp_at(layer, ...).map_frame(vaddr, frame@)
            }
        };
        res
    }

    #[allow(unused_parens)] // https://github.com/secure-foundations/verus/issues/230
    fn map_frame(&mut self, vaddr: usize, frame: MemRegionExec) -> (res: (Result<(),()>))
        requires
            old(self).inv(),
            old(self).accepted_mapping(0, old(self).memory.root(), 0, vaddr, frame@),
            vaddr < MAX_BASE,
    {
        proof { ambient_arith(); }
        self.map_frame_aux(0, self.memory.root_exec(), 0, vaddr, frame)
    }
}

}
