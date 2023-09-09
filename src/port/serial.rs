//! MIDI using HAL Serial

use embedded_hal::serial::{Write, Read};

use hal::serial::{Event, TxISR, RxISR, CommonPins, Listen};

use heapless::spsc::Queue;
use midi::{Packet, MidiError, Receive, Transmit, PacketList};

// TODO use DMA? https://github.com/stm32-rs/stm32f4xx-hal/blob/master/examples/rtic-serial-dma-rx-idle.rs

pub struct SerialMidi<UART: CommonPins> {
    pub uart: hal::serial::Serial<UART>,
    pub tx_fifo: Queue<u8, 64>,
    parser: midi::PacketParser,
    last_status: Option<u8>,
}

impl<UART> SerialMidi<UART> where
    UART: CommonPins,
    hal::serial::Serial<UART>: TxISR + Write<u8> + Listen,
{
    pub fn new(uart: hal::serial::Serial<UART>) -> Self {
        SerialMidi {
            uart,
            tx_fifo: Queue::new(),
            parser: midi::PacketParser::default(),
            last_status: None,
        }
    }

    pub fn flush(&mut self) -> Result<(), MidiError> {
        'write_bytes:
        loop {
            if self.uart.is_tx_empty() {
                if let Some(byte) = self.tx_fifo.dequeue() {
                    self.uart.write(byte)?;
                    continue 'write_bytes;
                } else {
                    self.uart.unlisten(Event::Txe)
                }
            } else {
                self.uart.listen(Event::Txe)
            }
            return Ok(());
        }
    }

    fn write_all(&mut self, payload: &[u8]) -> Result<(), MidiError> {
        for byte in payload {
            self.write_byte(*byte)?
        }
        Ok(())
    }

    fn write_byte(&mut self, byte: u8) -> Result<(), MidiError> {
        self.tx_fifo.enqueue(byte).map_err(|_| MidiError::BufferFull)?;
        Ok(())
    }
}

impl<UART> Receive for SerialMidi<UART> where
    UART: CommonPins,
    hal::serial::Serial<UART>: RxISR + Read<u8>,
{
    fn receive(&mut self) -> Result<Option<Packet>, MidiError> {
        if self.uart.is_rx_not_empty() {
            let byte = self.uart.read()?;
            let packet = self.parser.advance(byte)?;
            if let Some(packet) = packet {
                return Ok(Some(packet.with_cable_num(1)));
            }
        }
        Ok(None)
    }
}

impl<UART> Transmit for SerialMidi<UART> where
    UART: CommonPins,
    hal::serial::Serial<UART>: TxISR + Write<u8> + Listen,
{
    fn transmit(&mut self, packets: PacketList) -> Result<(), MidiError> {
        for packet in packets.iter() {
            let mut payload = packet.payload();

            if midi::is_channel_status(payload[0]) {
                // Apply MIDI "running status"
                if let Some(last_status) = self.last_status {
                    if payload[0] == last_status {
                        // same status as last time, chop out status byte
                        payload = &payload[1..];
                    } else {
                        // take note of new status
                        self.last_status = Some(payload[0])
                    }
                }
            } else {
                // non-repeatable status or no status (sysex)
                self.last_status = None
            }
            self.write_all(payload)?;
        }
        self.flush()?;
        Ok(())
    }
}
