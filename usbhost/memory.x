MEMORY
{
  /* Leave 16k for the default bootloader on the Trinket M0 */
  FLASH (rx) : ORIGIN = 0x00000000 + 16K, LENGTH = 256K - 16K
  RAM (xrw)  : ORIGIN = 0x20000000, LENGTH = 32K
}
_stack_start = ORIGIN(RAM) + LENGTH(RAM);

