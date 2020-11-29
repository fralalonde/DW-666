/* Linker script for the STM32F103C8T6 */
MEMORY
{
  FLASH : ORIGIN = 0x08000000, LENGTH = 64K

  RAM : ORIGIN = 0x20000000, LENGTH = 20K

  /* first 10k reserved for dumb heap alloc */
  /* RAM : ORIGIN = 0x20002800, LENGTH = 10K */
}