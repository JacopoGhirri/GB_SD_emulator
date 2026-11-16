//! OAM DMA (Pan Docs "OAM DMA Transfer").
//!
//! Writing `XX` to 0xFF46 copies 160 bytes from `XX00..XX9F` into OAM (0xFE00..0xFE9F), one byte
//! per M-cycle. The transfer starts one M-cycle *after* the one that follows the write: in the
//! M-cycle right after the write the CPU still sees normal memory (and a previous transfer, if
//! any, keeps running); from the next one the DMA owns the bus. While it runs the CPU can only
//! use HRAM and the I/O registers.
//!
//! This module only does the counting; [`crate::bus::Bus`] performs the actual copy because it
//! owns the memories involved.

/// Bytes in OAM, so bytes per transfer.
pub const OAM_DMA_LEN: u8 = 160;

/// M-cycles from the write M-cycle until the first byte is copied.
const START_DELAY: u8 = 2;

#[derive(Debug, Clone, Default)]
pub struct Dma {
    /// Source address of the running transfer (`page << 8`), valid while `active`.
    source: u16,
    /// Next byte of the running transfer.
    index: u8,
    active: bool,
    /// Source page of a transfer that has been requested but has not started yet.
    pending_page: u8,
    /// M-cycles left before the pending request takes over; 0 = nothing pending.
    start_in: u8,
}

impl Dma {
    pub fn new() -> Dma {
        Dma::default()
    }

    /// Request a transfer from page `page` (the value written to 0xFF46).
    pub fn start(&mut self, page: u8) {
        self.pending_page = page;
        self.start_in = START_DELAY;
    }

    /// True while the DMA owns the bus, i.e. while the CPU is restricted to HRAM/IO.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Advance one M-cycle. Returns `(source address, OAM index)` of the byte to copy now.
    pub fn tick(&mut self) -> Option<(u16, u8)> {
        let copy = if self.active {
            let item = (self.source + u16::from(self.index), self.index);
            self.index += 1;
            if self.index == OAM_DMA_LEN {
                self.active = false;
            }
            Some(item)
        } else {
            None
        };
        // A pending request takes over after this cycle's copy (the old transfer keeps running
        // during the start-up delay) and restarts from byte 0.
        if self.start_in > 0 {
            self.start_in -= 1;
            if self.start_in == 0 {
                self.source = source_address(self.pending_page);
                self.index = 0;
                self.active = true;
            }
        }
        copy
    }
}

/// On DMG, source pages 0xE0..=0xFF read the echo of WRAM, i.e. behave like 0xC0..=0xDF
/// (Pan Docs: the source can be 0x0000-0xDFFF; higher values mirror WRAM).
fn source_address(page: u8) -> u16 {
    let page = if page >= 0xE0 { page - 0x20 } else { page };
    u16::from(page) << 8
}

impl Dma {
    /// Save-state support (see `crate::state`).
    pub fn save_state(&self, w: &mut crate::state::StateWriter) {
        w.u16(self.source);
        w.u8(self.index);
        w.bool(self.active);
        w.u8(self.pending_page);
        w.u8(self.start_in);
    }

    pub fn load_state(
        &mut self,
        r: &mut crate::state::StateReader,
    ) -> Result<(), crate::state::StateError> {
        self.source = r.u16()?;
        self.index = r.u8()?;
        self.active = r.bool()?;
        self.pending_page = r.u8()?;
        self.start_in = r.u8()?;
        // `index` addresses OAM while a transfer runs, `source + index` must not overflow.
        if self.index > OAM_DMA_LEN || (self.active && self.index >= OAM_DMA_LEN) {
            return Err(crate::state::StateError::Corrupt("OAM DMA index"));
        }
        self.source &= 0xFF00;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_does_nothing() {
        let mut d = Dma::new();
        assert_eq!(d.tick(), None);
        assert!(!d.is_active());
    }

    #[test]
    fn first_byte_is_copied_two_cycles_after_the_write_cycle() {
        let mut d = Dma::new();
        d.start(0xC1); // the write M-cycle ends with this tick:
        assert_eq!(d.tick(), None);
        assert!(
            !d.is_active(),
            "CPU keeps the bus in the cycle after the write"
        );
        assert_eq!(d.tick(), None);
        assert!(
            d.is_active(),
            "bus owned from the second cycle after the write"
        );
        assert_eq!(d.tick(), Some((0xC100, 0)));
        assert_eq!(d.tick(), Some((0xC101, 1)));
    }

    #[test]
    fn copies_160_bytes_then_releases_the_bus() {
        let mut d = Dma::new();
        d.start(0x80);
        let mut copied = 0;
        for _ in 0..200 {
            if d.tick().is_some() {
                copied += 1;
            }
        }
        assert_eq!(copied, 160);
        assert!(!d.is_active());
    }

    #[test]
    fn restart_keeps_old_transfer_running_during_delay() {
        let mut d = Dma::new();
        d.start(0x10);
        d.tick();
        d.tick();
        d.tick(); // byte 0 of the first transfer
        d.start(0x20);
        assert_eq!(d.tick(), Some((0x1001, 1)), "old transfer continues");
        assert_eq!(d.tick(), Some((0x1002, 2)), "and once more before takeover");
        assert_eq!(
            d.tick(),
            Some((0x2000, 0)),
            "then the new one starts at byte 0"
        );
    }

    #[test]
    fn high_source_pages_mirror_wram() {
        assert_eq!(source_address(0xE5), 0xC500);
        assert_eq!(source_address(0xFF), 0xDF00);
        assert_eq!(source_address(0xDF), 0xDF00);
    }
}
