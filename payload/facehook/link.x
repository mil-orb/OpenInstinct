/* Face loader, in the 0xff padding at the end of the internal app region
   (0xf41ca-0xfffff in 19.10), below the nav payload at 0xfd700.
   Code and constants only. */
MEMORY { FREE (rx) : ORIGIN = 0x000f4200, LENGTH = 0x9500 }
ENTRY(face_entry)
SECTIONS
{
  .text : {
    KEEP(*(.text.face_entry))
    KEEP(*(.text.page_flags_entry))
    *(.text .text.*)
    *(.rodata .rodata.*)
  } > FREE
  .data : { *(.data .data.*) } > FREE
  .bss  : { *(.bss .bss.* COMMON) } > FREE
  /DISCARD/ : { *(.ARM.exidx*) *(.ARM.extab*) *(.comment) }
}
ASSERT(SIZEOF(.data) == 0, "payload must not have .data")
ASSERT(SIZEOF(.bss) == 0, "payload must not have .bss")
