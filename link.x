ENTRY(_start)

SECTIONS
{
    __firmware_start = 0x80000000;

    . = 0x80200000;

    .text : ALIGN(4K) {
        __text_start = .;
        *(.text.init)
        *(.text .text.*)
    }

    .rodata : ALIGN(4K) {
        __rodata_start = .;
        *(.rodata .rodata.*)
    }

    .data : ALIGN(4K) {
        __data_start = .;
        *(.data .data.*)
        __small_data_start = .;
        *(.sdata .sdata.*)
    }

    .eh_frame : ALIGN(8) {
        *(.eh_frame .eh_frame.*)
    }

    PROVIDE(__global_pointer$ = __small_data_start + 0x800);

    .bss (NOLOAD) : ALIGN(16) {
        __bss_start = .;
        *(.sbss .sbss.*)
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(8);
        __bss_end = .;
    }

    .stack (NOLOAD) : ALIGN(4K) {
        __stack_protector = .;
        . += 4K;
        __stack_bottom = .;
        . += 64K;
        __stack_top = .;
    }
}
