#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![cfg_attr(test, feature(const_type_name))]

extern crate alloc;

use core::arch::naked_asm;

use device_tree_parser::DeviceTreeParser;
use embedded_alloc::TlsfHeap;

mod console;
mod shutdown;
#[cfg(test)]
mod test;

#[global_allocator]
static HEAP: TlsfHeap = TlsfHeap::empty();

#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.init")]
extern "C" fn _start() -> ! {
    naked_asm!(
        // Initialize gp for relative addressing of small data
        ".option push",
        ".option norelax",
        "la gp, __global_pointer$",
        ".option pop",

        // Initialize stack pointer
        "la sp, __stack_top",

        // Clear .bss
        "la t0, __bss_start",
        "la t1, __bss_end",
        "1:",
        "bgeu t0, t1, 2f",
        "sd zero, 0(t0)",
        "addi t0, t0, 8",
        "j 1b",
        "2:",

        // Enable floating-point operations
        "li t0, 0x6000",
        "csrc sstatus, t0",
        "li t0, 0x2000",
        "csrs sstatus, t0",
        "csrw fcsr, zero",

        // Call main with a0 and a1 set by SBI
        "tail {main}",
        main = sym main,
    )
}

static LOGO: &str = r#"

(_)_ __(_)___ 
| | '__| / __|
| | |  | \__ \
|_|_|  |_|___/

"#;

#[unsafe(no_mangle)]
extern "C" fn main(hart_id: usize, dtb_address: usize) -> ! {
    println!("{LOGO}");
    println!("starting iris on hart {hart_id}, dtb_address={dtb_address}");

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
                    "{size_unit} of memory found at 0x{addr:016x}..0x{:016x}",
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
            "reserved: 0x{:016x}..0x{:016x} (0x{:016x})",
            res.address,
            res.address + res.size,
            res.size
        );
    }

    shutdown::init(&tree).expect("global shutdown device not initialized");

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
