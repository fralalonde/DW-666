#[allow(unused)]
pub mod addr;
#[allow(unused)]
pub mod ctrl_pipe;
#[allow(unused)]
pub mod ext_reg;
#[allow(unused)]
pub mod pck_size;
#[allow(unused)]
pub mod status_bk;
#[allow(unused)]
pub mod status_pipe;

mod table;
mod regs;

pub use table::PipeTable;
pub use addr::Addr;

use ctrl_pipe::CtrlPipe;
use ext_reg::ExtReg;
use pck_size::PckSize;
use status_bk::StatusBk;
use status_pipe::StatusPipe;

use crate::usb_host::{Endpoint, RequestCode, RequestDirection, RequestType, SetupPacket, Toggle, TransferType, WValue};

use RequestDirection::{DeviceToHost, HostToDevice};
use crate::error::PipeErr;
use crate::pipe::regs::PipeRegs;

// Maximum time to wait for a control request with data to finish. cf §9.2.6.1 of USB 2.0.
const SETUP_TIMEOUT: u64 = 500;

// 5 milliseconds
const STATUS_TIMEOUT: u64 = 50; // 50 milliseconds

// How many times to retry a transaction that has transient errors.
const NAK_LIMIT: usize = 15;

// TODO: hide regs/desc fields. Needed right now for init_pipe0.
pub(crate) struct Pipe<'a, 'b> {
    pub(crate) num: usize,
    pub(crate) regs: PipeRegs<'b>,
    pub(crate) desc: &'a mut PipeDesc,
    pub(crate) millis: fn() -> u64,
}

impl Pipe<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn control_transfer(
        &mut self,
        endpoint: &mut Endpoint,
        req_type: RequestType,
        req_code: RequestCode,
        w_value: WValue,
        w_index: u16,
        buf: Option<&mut [u8]>,
    ) -> Result<usize, PipeErr> {
        debug!("USB Pipe[{}] CTRL Transfer [{:?}]", self.num, req_type);

        // SETUP stage
        let buflen = buf.as_ref().map_or(0, |b| b.len() as u16);
        let mut setup_packet = SetupPacket {
            bm_request_type: req_type,
            b_request: req_code,
            w_value,
            w_index,
            w_length: buflen,
        };

        self.ctl_transfer(endpoint, PToken::Setup, Some(setup_packet))?;
        debug!("stppck {:?}", setup_packet);

        // DATA stage (optional)
        let mut transfer_len = 0;
        if let Some(buffer) = buf {
            // TODO data stage has up to 5s (in 500ms per-packet chunks) to complete. cf §9.2.6.4 of USB 2.0
            transfer_len = match req_type.direction() {
                DeviceToHost => self.in_transfer(endpoint, buffer)?,
                HostToDevice => self.out_transfer(endpoint, buffer)?,
            };
        }

        // STATUS stage - has up to 50ms to complete. cf §9.2.6.4 of USB 2.0
        let token = match req_type.direction() {
            DeviceToHost => PToken::Out,
            HostToDevice => PToken::In,
        };
        self.ctl_transfer(endpoint, token, None)?;

        Ok(transfer_len)
    }

    fn ctl_transfer(&mut self, endpoint: &mut Endpoint, token: PToken, mut setup_packet: Option<SetupPacket>) -> Result<(), PipeErr> {
        debug!("USB Pipe[{}] CTL Transfer", self.num);
        let mut len = 0;
        if let Some(mut setup_packet) = setup_packet {
            self.desc.bank0.addr.write(|w| unsafe { w.addr().bits(&mut setup_packet as *mut SetupPacket as u32) });
            len = core::mem::size_of::<SetupPacket>() as u16
        }
        self.desc.bank0.pcksize.modify(|_, w| {
            unsafe { w.byte_count().bits(len) };
            unsafe { w.multi_packet_size().bits(0) }
        });
        // TODO: status stage has up to 50ms to complete. cf §9.2.6.4 of USB 2.0
        self.sync_tx(endpoint, token)
    }

    pub(crate) fn in_transfer(&mut self, endpoint: &mut Endpoint, read_buf: &mut [u8]) -> Result<usize, PipeErr> {
        debug!("USB Pipe[{}] IN Transfer", self.num);
        self.desc.bank0.pcksize.modify(|_, pcksize| unsafe {
            pcksize.byte_count().bits(read_buf.len() as u16);
            pcksize.multi_packet_size().bits(0)
        });

        // Read until we get a short packet or the buffer is full
        let mut total_bytes = 0;
        loop {
            // Move the buffer pointer forward as we get data.
            self.desc.bank0.addr.write(|bank0| unsafe { bank0.addr().bits(read_buf.as_mut_ptr() as u32 + total_bytes as u32) });
            self.regs.statusclr.write(|w| w.bk0rdy().set_bit());

            self.sync_tx(endpoint, PToken::In)?;

            let byte_count = self.desc.bank0.pcksize.read().byte_count().bits();
            total_bytes += byte_count as usize;

            // short read?
            if byte_count < endpoint.max_packet_size { break; }
            if total_bytes >= read_buf.len() { break; }
        }
        // TODO return subslice of buffer for safe short packet
        Ok(total_bytes)
    }

    pub(crate) fn out_transfer(&mut self, ep: &mut Endpoint, buf: &[u8]) -> Result<usize, PipeErr> {
        trace!("USB Pipe[{}] OUT Transfer ", self.num);
        self.desc.bank0.pcksize.modify(|_, w| {
            unsafe { w.byte_count().bits(buf.len() as u16) };
            unsafe { w.multi_packet_size().bits(0) }
        });

        let mut bytes_sent = 0;
        while bytes_sent < buf.len() {
            self.desc.bank0.addr.write(|bank0| unsafe { bank0.addr().bits(buf.as_ptr() as u32 + bytes_sent as u32) });
            self.sync_tx(ep, PToken::Out)?;
            let sent = self.desc.bank0.pcksize.read().byte_count().bits() as usize;
            bytes_sent += sent;
            trace!("USB Pipe[{}] Sent {} of {} bytes", self.num, bytes_sent, buf.len());
        }
        Ok(bytes_sent)
    }

    // fn toggle_ep(&mut self, endpoint: &mut Endpoint, token: PToken) {
    //     let toggle = match token {
    //         PToken::In => endpoint.flip_toggle_in(),
    //         PToken::Out => endpoint.flip_toggle_out(),
    //         PToken::Setup => Toggle::Data0,
    //     };
    //     self.toggle_set(toggle);
    // }
    //
    // fn toggle_set(&self, dtgl: Toggle) {
    //     match dtgl {
    //         Toggle::Data0 => {
    //             trace!("USB Pipe[{}] DTGL DATA0", self.num);
    //             self.regs.statusclr.write(|w| unsafe { w.bits(1) });
    //         }
    //         Toggle::Data1 => {
    //             trace!("USB Pipe[{}] DTGL DATA1", self.num);
    //             self.regs.statusset.write(|w| w.dtgl().set_bit());
    //         }
    //     }
    // }

    fn sync_tx(&mut self, endpoint: &mut Endpoint, token: PToken) -> Result<(), PipeErr> {
        trace!("USB Pipe[{}] Initiating Transfer", self.num);
        self.transfer_start(endpoint, token);

        let until = (self.millis)() + SETUP_TIMEOUT;
        let mut naks = 0;
        loop {
            match self.transfer_status(token) {
                Ok(true) => {
                    // match token {
                    //     PToken::In => { endpoint.flip_toggle_in(); }
                    //     PToken::Out => { endpoint.flip_toggle_out(); }
                    //     _ => {}
                    // };
                    return Ok(());
                }
                Err(err) => {
                    warn!("USB Pipe[{}] Transfer error [{:?}]",  self.num, err);
                    match err {
                        PipeErr::DataToggle => {}
                        // self.toggle_ep(endpoint, token),

                        // Flow error means we got a NAK, which means there's no data. cf §32.8.7.5 of SAM D21 data sheet.
                        PipeErr::Flow if endpoint.transfer_type == TransferType::Interrupt =>
                            return Err(PipeErr::Flow),
                        PipeErr::Stall =>
                            return Err(PipeErr::Stall),
                        _ => {
                            naks += 1;
                            if naks > NAK_LIMIT {
                                return Err(PipeErr::NaksExceeded);
                            }
                        }
                    }
                }
                _ => {
                    let now = (self.millis)();
                    // trace!("USB Pipe[{}] now [{}]", self.num, now);
                    if now >= until {
                        trace!("USB Pipe[{}] Timeout Transfer", self.num);
                        return Err(PipeErr::SwTimeout);
                    }
                    cortex_m::asm::delay(80000);
                }
            }
        }
    }

    fn transfer_start(&mut self, endpoint: &mut Endpoint, token: PToken) {
        self.regs.cfg.modify(|_, w| unsafe { w.ptoken().bits(token as u8) });
        self.regs.intflag.modify(|_, w| w.trfail().set_bit());
        self.regs.intflag.modify(|_, w| w.perr().set_bit());

        match token {
            PToken::Setup => {
                self.regs.intflag.write(|w| w.txstp().set_bit());
                self.regs.statusset.write(|w| w.bk0rdy().set_bit());

                // Toggles should be 1 at end of setup cf §8.6.1 of USB 2.0.
                // self.toggle_set(Toggle::Data0);
                // endpoint.in_toggle = Toggle::Data1;
                // endpoint.out_toggle = Toggle::Data1;
            }
            PToken::In => {
                self.regs.statusclr.write(|w| w.bk0rdy().set_bit());
                // self.toggle_set(endpoint.in_toggle);
            }
            PToken::Out => {
                self.regs.intflag.write(|w| w.trcpt0().set_bit());
                self.regs.statusset.write(|w| w.bk0rdy().set_bit());
                // self.toggle_set(endpoint.out_toggle);
            }
        }
        self.regs.statusclr.write(|w| w.pfreeze().set_bit());

        self.log_regs()
    }

    fn transfer_status(&mut self, token: PToken) -> Result<bool, PipeErr> {
        let intflag = self.regs.intflag.read();
        let status_pipe = self.desc.bank0.status_pipe.read();

        debug!("intflag {:?} status {:?}", intflag.bits(),  status_pipe.bits());
        match token {
            PToken::Setup if intflag.txstp().bit_is_set() => {
                self.regs.intflag.write(|w| w.txstp().set_bit());
                return Ok(true);
            }
            PToken::In | PToken::Out if intflag.trcpt0().bit_is_set() => {
                self.regs.intflag.write(|w| w.trcpt0().set_bit());
                return Ok(true);
            }
            _ => {}
        };

        // trace!("USB Pipe[{}] Reading Pipe Errors", self.num);
        if self.desc.bank0.status_bk.read().errorflow().bit_is_set() {
            Err(PipeErr::Flow)
        } else if status_pipe.touter().bit_is_set() {
            Err(PipeErr::HwTimeout)
        } else if intflag.stall().bit_is_set() {
            Err(PipeErr::Stall)
        } else if status_pipe.dtgler().bit_is_set() {
            Err(PipeErr::DataToggle)
        } else if intflag.trfail().bit_is_set() {
            self.regs.intflag.write(|w| w.trfail().set_bit());
            Err(PipeErr::TransferFail)
        } else {
            // not done yet
            Ok(false)
        }
    }

    fn log_regs(&self) {
        // Pipe regs
        let cfg = self.regs.cfg.read().bits();
        let bin = self.regs.binterval.read().bits();
        let sts = self.regs.status.read().bits();
        let ifl = self.regs.intflag.read().bits();
        trace!(
            "p{}: cfg: {:x}, bin: {:x}, stat: {:x}, int: {:x}",
            self.num,
            cfg,
            bin,
            sts,
            ifl
        );

        // Bank regs
        let adr = self.desc.bank0.addr.read().bits();
        let pks = self.desc.bank0.pcksize.read().bits();
        let ext = self.desc.bank0.extreg.read().bits();
        let sbk = self.desc.bank0.status_bk.read().bits();
        let hcp = self.desc.bank0.ctrl_pipe.read().bits();
        let spi = self.desc.bank0.status_pipe.read().bits();
        trace!(
            "USB Pipe {}: addr: {:x}, pcks: {:x}, extr: {:x}, stbk: {:x}, ctrl: {:x}, stat: {:x}",
            self.num,
            adr,
            pks,
            ext,
            sbk,
            hcp,
            spi
        );
    }
}

// TODO: merge into SVD for pipe cfg register.
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum PToken {
    Setup = 0x0,
    In = 0x1,
    Out = 0x2,
    // _Reserved = 0x3,
}

// TODO: merge into SVD for pipe cfg register.
#[allow(unused)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum PipeType {
    Disabled = 0x0,
    Control = 0x1,
    ISO = 0x2,
    Bulk = 0x3,
    Interrupt = 0x4,
    Extended = 0x5,
    // _Reserved0 = 0x06,
    // _Reserved1 = 0x07,
}

impl From<TransferType> for PipeType {
    fn from(v: TransferType) -> Self {
        match v {
            TransferType::Control => Self::Control,
            TransferType::Isochronous => Self::ISO,
            TransferType::Bulk => Self::Bulk,
            TransferType::Interrupt => Self::Interrupt,
        }
    }
}

// §32.8.7.1
pub(crate) struct PipeDesc {
    pub bank0: BankDesc,
    // can be used in ping-pong mode (SAMD USB dual buffering)
    pub bank1: BankDesc,
}

// 2 banks: 32 bytes per pipe.
impl PipeDesc {
    pub fn new() -> Self {
        Self {
            bank0: BankDesc::new(),
            bank1: BankDesc::new(),
        }
    }
}

#[repr(C, packed)]
pub(crate) struct BankDesc {
    pub addr: Addr,
    pub pcksize: PckSize,
    pub extreg: ExtReg,
    pub status_bk: StatusBk,
    _reserved0: u8,
    pub ctrl_pipe: CtrlPipe,
    pub status_pipe: StatusPipe,
    _reserved1: u8,
}

impl BankDesc {
    fn new() -> Self {
        Self {
            addr: Addr::from(0),
            pcksize: PckSize::from(0),
            extreg: ExtReg::from(0),
            status_bk: StatusBk::from(0),
            _reserved0: 0,
            ctrl_pipe: CtrlPipe::from(0),
            status_pipe: StatusPipe::from(0),
            _reserved1: 0,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn bank_desc_sizes() {
        assert_eq!(core::mem::size_of::<Addr>(), 4, "Addr register size.");
        assert_eq!(core::mem::size_of::<PckSize>(), 4, "PckSize register size.");
        assert_eq!(core::mem::size_of::<ExtReg>(), 2, "ExtReg register size.");
        assert_eq!(
            core::mem::size_of::<StatusBk>(),
            1,
            "StatusBk register size."
        );
        assert_eq!(
            core::mem::size_of::<CtrlPipe>(),
            2,
            "CtrlPipe register size."
        );
        assert_eq!(
            core::mem::size_of::<StatusPipe>(),
            1,
            "StatusPipe register size."
        );

        // addr at 0x00 for 4
        // pcksize at 0x04 for 4
        // extreg at 0x08 for 2
        // status_bk at 0x0a for 2
        // ctrl_pipe at 0x0c for 2
        // status_pipe at 0x0e for 1
        assert_eq!(
            core::mem::size_of::<BankDesc>(),
            16,
            "Bank descriptor size."
        );
    }

    #[test]
    fn bank_desc_offsets() {
        let bd = BankDesc::new();
        let base = &bd as *const _ as usize;

        assert_offset("Addr", &bd.addr, base, 0x00);
        assert_offset("PckSize", &bd.pcksize, base, 0x04);
        assert_offset("ExtReg", &bd.extreg, base, 0x08);
        assert_offset("StatusBk", &bd.status_bk, base, 0x0a);
        assert_offset("CtrlPipe", &bd.ctrl_pipe, base, 0x0c);
        assert_offset("StatusPipe", &bd.status_pipe, base, 0x0e);
    }

    #[test]
    fn pipe_desc_size() {
        assert_eq!(core::mem::size_of::<PipeDesc>(), 32);
    }

    #[test]
    fn pipe_desc_offsets() {
        let pd = PipeDesc::new();
        let base = &pd as *const _ as usize;

        assert_offset("Bank0", &pd.bank0, base, 0x00);
        assert_offset("Bank1", &pd.bank1, base, 0x10);
    }

    fn assert_offset<T>(name: &str, field: &T, base: usize, offset: usize) {
        let ptr = field as *const _ as usize;
        assert_eq!(ptr - base, offset, "{} register offset.", name);
    }
}
