

use defmt_rtt as _; // global logger

extern crate panic_probe as _;

defmt::timestamp!("{=u64}", {
    time::now_millis()
});

#[defmt::panic_handler]
fn panic() -> ! {
    cortex_m::asm::udf()
}