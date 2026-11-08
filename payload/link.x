/* Payload lives in the 0xff padding at the end of the internal app region
   (0xf41ca-0xfffff in the 19.10 image): the top 11 KB (0xfd400), above the face loader
   (0xf4400, which may grow up to 0xfd000). It moved here from 0xf4400 when the
   nav screen layout outgrew the 7 KB below the old face loader at 0xf6000.
   Code and constants only. */
MEMORY { FREE (rx) : ORIGIN = 0x000fd400, LENGTH = 0x2c00 }
ENTRY(nav_entry)
SECTIONS
{
  .text : {
    KEEP(*(.text.nav_entry)) KEEP(*(.text.evt_entry)) KEEP(*(.text.alert_entry)) KEEP(*(.text.tick_entry)) KEEP(*(.text.vibe_entry))
    *(.text .text.*)
    *(.rodata .rodata.*)
  } > FREE
  .data : { *(.data .data.*) } > FREE
  .bss  : { *(.bss .bss.* COMMON) } > FREE
  /DISCARD/ : { *(.ARM.exidx*) *(.ARM.extab*) *(.comment) }
}
ASSERT(SIZEOF(.data) == 0, "payload must not have .data")
ASSERT(SIZEOF(.bss) == 0, "payload must not have .bss")
