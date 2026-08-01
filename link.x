ENTRY(_start)

SECTIONS
{
    __firmware_start = 0x80000000;

    . = 0x80200000;
    __link_base_addr = .;

    .text.init : ALIGN(4K) {
        *(.text.init)
    }

    .bss.init (NOLOAD) : ALIGN(4K) {
        __init_bss_start = .;
        __init_page_table_l2 = .;
        . += 4K;
        __init_page_table_l1_trampoline = .;
        . += 4K;
        __init_page_table_l0_trampoline = .;
        . += 4K;
        __init_page_table_l1_kernel = .;
        . += 4K;
        __init_page_table_l0_kernel = .;
        . += 4K;
        __init_bss_end = .;
    }

    __kernel_start_phys = .;
    __kernel_start = 0xffffffc000000000;
    __kernel_offset = __kernel_start - __kernel_start_phys;
    . = __kernel_start;

    .text : AT(. - __kernel_offset) ALIGN(4K) {
        __text_start = .;
        *(.text .text.*)
    }

    .rodata : AT(. - __kernel_offset) ALIGN(4K) {
        __rodata_start = .;
        *(.rodata .rodata.*)
    }

    .data : AT(. - __kernel_offset) ALIGN(4K) {
        __data_start = .;
        *(.data .data.*)
        __small_data_start = .;
        *(.sdata .sdata.*)
    }

    .eh_frame : AT(. - __kernel_offset) ALIGN(8) {
        *(.eh_frame .eh_frame.*)
    }

    PROVIDE(__global_pointer$ = __small_data_start + 0x800);

    .bss (NOLOAD) : AT(. - __kernel_offset) ALIGN(16) {
        __bss_start = .;
        # __bss_start_phys = . - __kernel_offset;
        *(.sbss .sbss.*)
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(8);
        __bss_end = .;
        # __bss_end_phys = . - __kernel_offset;
    }

    .stack (NOLOAD) : AT(. - __kernel_offset) ALIGN(4K) {
        __stack_protector = .;
        . += 4K;
        __stack_bottom = .;
        . += 64K;
        __stack_top = .;
    }

    . = ALIGN(2M);
    __kernel_end = .;
}

ASSERT(SIZEOF(.text.init) <= 4K, "boot trampoline should not exceed one page");
ASSERT(__kernel_end - __kernel_start <= 2M, "kernel size should not exceed 2M")
