/* STM32F411RE: 512 KB flash at 0x08000000, 128 KB SRAM at 0x20000000.
   Datasheet DS10314, Table 4 (memory mapping). cortex-m-rt's link.x places
   the vector table at the start of FLASH and expects this file to exist. */
MEMORY
{
  FLASH : ORIGIN = 0x08000000, LENGTH = 512K
  RAM   : ORIGIN = 0x20000000, LENGTH = 128K
}
