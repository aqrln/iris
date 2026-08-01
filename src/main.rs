#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![feature(debug_closure_helpers)]
#![cfg_attr(test, feature(const_type_name))]
#![test_runner(crate::test::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use core::arch::naked_asm;

use alloc::vec;
use device_tree_parser::DeviceTreeParser;
use embedded_alloc::TlsfHeap;

use crate::mmu::{
    MemoryManager, PagePermissions,
    addr::{AddressRange, PageType},
};

mod console;
mod mmu;
mod shutdown;
#[cfg(test)]
mod test;

#[global_allocator]
static HEAP: TlsfHeap = TlsfHeap::empty();

unsafe extern "C" {
    static __firmware_start: u8;
    static __text_start: u8;
    static __rodata_start: u8;
    static __data_start: u8;
    static __stack_protector: u8;
    static __stack_bottom: u8;
    static __stack_top: u8;
}

#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.init")]
extern "C" fn _start() -> ! {
    naked_asm!(
        // Load the DTB size while we're still in Bare addressing mode.
        "lwu a2, 4(a1)",

        // Store the real physical address of the beginning of the kernel
        // so we can translate the physical addresses later in Rust if we're
        // loaded elsewhere than __link_base_addr. All memory accesses in the
        // assembly boot trampoline are either PC-relative or reference virtual
        // absolute addresses in the upper canonical half, so they need no translation.
        "la a3, __link_base_addr",
        "la a4, __kernel_start_phys",

        // Clear the boot page tables
        "la t0, __init_bss_start",
        "la t1, __init_bss_end",
        "1:",
        "bgeu t0, t1, 2f",
        "sd zero, 0(t0)",
        "addi t0, t0, 8",
        "j 1b",
        "2:",

        // Identity map the boot trampoline page.
        "srli t0, a3, 30",
        "andi t0, t0, 0x1ff", // t0 = vpn2(a3)
        "la t1, __init_page_table_l2",
        "li t3, 8",
        "mul t0, t0, t3",
        "add t0, t0, t1",
        "la t1, __init_page_table_l1_trampoline",
        "mv t4, t1",
        "srli t4, t4, 12", // t4 = ppn(t1)
        "slli t4, t4, 10", // ppn to pte
        "ori t4, t4, 1", // flags: V
        "sd t4, 0(t0)",
        "srli t0, a3, 21",
        "andi t0, t0, 0x1ff", // t0 = vpn1(a3)
        "mul t0, t0, t3",
        "add t0, t0, t1",
        "la t1, __init_page_table_l0_trampoline",
        "mv t4, t1",
        "srli t4, t4, 12", // t4 = ppn(t1)
        "slli t4, t4, 10", // ppn to pte
        "ori t4, t4, 1", // flags: V
        "sd t4, 0(t0)",
        "srli t0, a3, 12",
        "andi t0, t0, 0x1ff", // t0 = vpn0(a3)
        "mul t0, t0, t3",
        "add t0, t0, t1",
        "srli t1, a3, 12", // t1 = ppn(a3)
        "slli t1, t1, 10", // ppn to pte
        "ori t1, t1, 11", // flags: VRX
        "sd t1, 0(t0)",

        // Map the kernel up above.
        // Kernel's virtual base address must be aligned to 2 MB
        // and kernel size must not exceed 2 MB.
        // We map individual 4K pages and not a single 2M page because
        // the physical address might not be aligned to 2M.
        "ld t2, __kernel_start_ptr",
        "srli t0, t2, 30",
        "andi t0, t0, 0x1ff", // t0 = vpn2(t2)
        "mul t0, t0, t3",
        "la t1, __init_page_table_l2",
        "add t0, t0, t1",
        "la t1, __init_page_table_l1_kernel",
        "mv t4, t1",
        "srli t4, t4, 12", // t4 = ppn(t1)
        "slli t4, t4, 10", // ppn to pte
        "ori t4, t4, 1", // flags: V
        "sd t4, 0(t0)",
        "srli t0, t2, 21",
        "andi t0, t0, 0x1ff", // t0 = vpn1(t2)
        "mul t0, t0, t3",
        "add t0, t0, t1",
        "la t1, __init_page_table_l0_kernel",
        "mv t4, t1",
        "srli t4, t4, 12", // t4 = ppn(t1)
        "slli t4, t4, 10", // ppn to pte
        "ori t4, t4, 1", // flags: V
        "sd t4, 0(t0)",
        "la t0, __kernel_start_phys",
        "srli t0, t0, 12",
        "addi t2, t0, 512",
        "3:", // loop over PTEs, t1 = &pte, t0 = ppn, t2 = max_ppn + 1
        "bgeu t0, t2, 4f",
        "slli t3, t0, 10",
        "ori t3, t3, 0xf", // flags: VRWX
        "sd t3, 0(t1)",
        "addi t0, t0, 1",
        "addi t1, t1, 8",
        "j 3b",
        "4:",

        // Enable Sv39 virtual memory
        "sfence.vma zero, zero",
        "la t0, __init_page_table_l2",
        "srli t0, t0, 12",
        "li t1, 8",
        "slli t1, t1, 60",
        "or t0, t0, t1",
        "csrw satp, t0",

        // Initialize gp for relative addressing of small data
        ".option push",
        ".option norelax",
        "ld gp, __global_pointer_ptr",
        ".option pop",

        // Initialize stack pointer
        "ld sp, __stack_top_ptr",

        // Clear .bss
        "ld t0, __bss_start_ptr",
        "ld t1, __bss_end_ptr",
        "5:",
        "bgeu t0, t1, 6f",
        "sd zero, 0(t0)",
        "addi t0, t0, 8",
        "j 5b",
        "6:",

        // Enable floating-point operations
        "li t0, 0x6000",
        "csrc sstatus, t0",
        "li t0, 0x2000",
        "csrs sstatus, t0",
        "csrw fcsr, zero",

        // Call main with a0 and a1 set by SBI and a2-a4 by us
        "ld t0, __main_ptr",
        "jr t0",

        // Pointers to far symbols that can't be addressed relative to PC
        ".balign 8",
        "__kernel_start_ptr: .dword __kernel_start",
        "__global_pointer_ptr: .dword __global_pointer$",
        "__stack_top_ptr: .dword __stack_top",
        "__bss_start_ptr: .dword __bss_start",
        "__bss_end_ptr: .dword __bss_end",
        "__main_ptr: .dword {main}",

        main = sym main,
    )
}

static LOGO: &str = r#"

(_)_ __(_)___ 
| | '__| / __|
| | |  | \__ \
|_|_|  |_|___/

"#;

extern "C" fn main(
    hart_id: usize,
    dtb_address: usize,
    dtb_size_be: usize,
    load_address: usize,
    kernel_start_phys: usize,
) -> ! {
    let dtb_size = u32::from_be(dtb_size_be as _) as usize;

    println!("{LOGO}");
    println!(
        "starting iris on hart {hart_id}, dtb_address={dtb_address:#x}, dtb_size={dtb_size:#x}, load_address={load_address:#x}, kernel_start_phys={kernel_start_phys:#x}"
    );

    unsafe {
        embedded_alloc::init!(HEAP, 1024 * 1024);
    };

    let dtb_magic = dtb_address as *const u32;
    assert_eq!(u32::from_be(unsafe { *dtb_magic }), 0xd00dfeed);

    let dtb_size = u32::from_be(unsafe { *dtb_magic.add(1) });
    let dtb_data = unsafe { core::slice::from_raw_parts(dtb_address as *const u8, dtb_size as _) };
    let dtp = DeviceTreeParser::new(dtb_data);

    let tree = dtp.parse_tree().expect("device tree must be valid");
    // println!("\nDevice tree:\n{tree}");

    for node in tree.iter_nodes() {
        if node.prop_string("device_type") == Some("memory") {
            let addrs = node
                .translate_reg_addresses(Some(&tree))
                .expect("register property of the memory device must be valid");
            for (addr, size) in addrs {
                let size_unit = match size {
                    x if x >= 1024 * 1024 * 1024 => format_args!("{} GB", x / 1024 / 1024 / 1024),
                    x if x >= 1024 * 1024 => format_args!("{} MB", x / 1024 / 1024),
                    x if x >= 1024 => format_args!("{} KB", x / 1024),
                    x => format_args!("{} B", x.clone()),
                };
                println!(
                    "{size_unit} of memory found at {addr:#016x}..{:#016x}",
                    addr + size
                );
            }
        }
    }

    for res in dtp
        .parse_memory_reservations()
        .expect("memory reservations must be valid")
    {
        println!(
            "reserved: {:#016x}..{:#016x} ({:#016x})",
            res.address,
            res.address + res.size,
            res.size
        );
    }

    let mut mm = MemoryManager::new_with_global_mappings(vec![
        (
            (&raw const __text_start..&raw const __rodata_start).into(),
            PagePermissions::READ | PagePermissions::EXECUTE,
        ),
        (
            (&raw const __rodata_start..&raw const __data_start).into(),
            PagePermissions::READ,
        ),
        (
            (&raw const __data_start..&raw const __stack_protector).into(),
            PagePermissions::READ | PagePermissions::WRITE,
        ),
        (
            (&raw const __stack_bottom..&raw const __stack_top).into(),
            PagePermissions::READ | PagePermissions::WRITE,
        ),
    ])
    .expect("failed to map kernel address space");

    mm.identity_map_kernel(
        AddressRange::from(dtb_data.as_ptr_range()).with_aligned_end(PageType::Small),
        PagePermissions::READ,
    )
    .expect("failed to map dtb")
    .do_not_flush();

    unsafe {
        mm.enable_virtual_memory();
    }

    shutdown::init(&tree, &mut mm).expect("global shutdown device not initialized");

    #[cfg(test)]
    test_main();

    loop {
        riscv::asm::wfi();
    }
}

#[panic_handler]
fn on_panic(info: &core::panic::PanicInfo) -> ! {
    unsafe { riscv::register::sstatus::clear_sie() };

    match info.location() {
        Some(location) => println!("kernel panic at {}: {}", location, info.message()),
        None => println!("kernel panic: {}", info.message()),
    }

    shutdown::get().shutdown_failure();
}
