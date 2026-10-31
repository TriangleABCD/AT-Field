/* SPDX-License-Identifier: Apache-2.0 */
/* AT Field — placeholder linker script.
 * 0x8000_0000 is the QEMU `virt` RAM base, used here only to get a linkable
 * image. The real load address will be fixed once the integration with the
 * host firmware is decided. */

ENTRY(_start)

SECTIONS
{
    . = 0x80000000;

    .text : {
        *(.text.entry)
        *(.text .text.*)
        . = ALIGN(8);
    }

    .rodata : {
        *(.rodata .rodata.*)
        . = ALIGN(8);
    }

    .data : {
        *(.data .data.*)
        . = ALIGN(8);
    }

    .bss : {
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(8);
    }
}
