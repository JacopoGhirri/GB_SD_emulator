//! CGB VRAM DMA: general-purpose DMA (GDMA) and HBlank DMA (HDMA), registers 0xFF51..=0xFF55
//! (Pan Docs "VRAM DMA Transfers"). This struct only holds the register state and the transfer
//! bookkeeping; the bus performs the actual byte copies and stalls the CPU.

pub const REG_HDMA1: u16 = 0xFF51; // source high
pub const REG_HDMA2: u16 = 0xFF52; // source low (low 4 bits ignored)
pub const REG_HDMA3: u16 = 0xFF53; // destination high (only bits 0-4 used, VRAM is at 0x8000)
pub const REG_HDMA4: u16 = 0xFF54; // destination low (low 4 bits ignored)
pub const REG_HDMA5: u16 = 0xFF55; // length / mode / start

/// Bytes moved per block: one GDMA block, or one HBlank.
pub const BLOCK_BYTES: u16 = 16;

#[derive(Debug, Clone, Default)]
pub struct Hdma {
    /// Source address, low 4 bits always 0.
    src: u16,
    /// Destination offset inside VRAM (0..=0x1FF0), low 4 bits always 0.
    dst: u16,
    /// Blocks still to copy minus one (the value HDMA5 bits 0-6 read back as).
    remaining: u8,
    /// An HBlank transfer is running (one block per HBlank).
    hblank_active: bool,
}

/// What a write to HDMA5 asks the bus to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Start {
    /// Nothing to copy now (register write, cancel, or an HBlank transfer that waits).
    Nothing,
    /// General-purpose DMA: copy this many 16-byte blocks immediately, stalling the CPU.
    General(u16),
    /// HBlank DMA started: copy the first block immediately if the PPU is already in HBlank,
    /// otherwise wait for the next one.
    HBlank,
}

impl Hdma {
    pub fn new() -> Hdma {
        Hdma {
            // Registers read back 0xFF after the CGB boot ROM (nothing written yet).
            src: 0xFFF0,
            dst: 0x1FF0,
            remaining: 0x7F,
            hblank_active: false,
        }
    }

    pub fn write(&mut self, addr: u16, value: u8) -> Start {
        match addr {
            REG_HDMA1 => self.src = (self.src & 0x00F0) | u16::from(value) << 8,
            REG_HDMA2 => self.src = (self.src & 0xFF00) | u16::from(value & 0xF0),
            REG_HDMA3 => self.dst = (self.dst & 0x00F0) | u16::from(value & 0x1F) << 8,
            REG_HDMA4 => self.dst = (self.dst & 0x1F00) | u16::from(value & 0xF0),
            REG_HDMA5 => return self.write_control(value),
            _ => {}
        }
        Start::Nothing
    }

    fn write_control(&mut self, value: u8) -> Start {
        if self.hblank_active {
            // Writing with bit 7 clear while an HBlank transfer runs cancels it; the
            // remaining length stays readable. Writing with bit 7 set just restarts it below.
            if value & 0x80 == 0 {
                self.hblank_active = false;
                return Start::Nothing;
            }
        }
        self.remaining = value & 0x7F;
        if value & 0x80 == 0 {
            Start::General(u16::from(self.remaining) + 1)
        } else {
            self.hblank_active = true;
            Start::HBlank
        }
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            // HDMA1-4 are write-only.
            REG_HDMA1..=REG_HDMA4 => 0xFF,
            // Bit 7 is 0 while an HBlank transfer is active, 1 when none is (finished or
            // cancelled); bits 0-6 are the blocks left minus one (0x7F when finished).
            REG_HDMA5 => (u8::from(!self.hblank_active) << 7) | self.remaining,
            _ => 0xFF,
        }
    }

    pub fn hblank_active(&self) -> bool {
        self.hblank_active
    }

    /// Take the next block's (source, VRAM offset) and advance. Ends the transfer after the last
    /// block (HDMA5 then reads 0xFF). Used for both GDMA and HBlank DMA.
    pub fn next_block(&mut self) -> (u16, u16) {
        let block = (self.src, self.dst);
        self.src = self.src.wrapping_add(BLOCK_BYTES);
        self.dst = (self.dst + BLOCK_BYTES) & 0x1FFF;
        if self.remaining == 0 {
            self.remaining = 0x7F;
            self.hblank_active = false;
        } else {
            self.remaining -= 1;
        }
        block
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn programmed() -> Hdma {
        let mut h = Hdma::new();
        h.write(REG_HDMA1, 0xC1);
        h.write(REG_HDMA2, 0x2F); // low nibble ignored
        h.write(REG_HDMA3, 0xFA); // only bits 0-4 used
        h.write(REG_HDMA4, 0x3F);
        h
    }

    #[test]
    fn address_registers_mask_their_unused_bits() {
        let mut h = programmed();
        assert_eq!(h.next_block(), (0xC120, 0x1A30));
    }

    #[test]
    fn general_dma_reports_block_count_and_reads_back_ff_when_done() {
        let mut h = programmed();
        assert_eq!(h.write(REG_HDMA5, 0x02), Start::General(3));
        for _ in 0..3 {
            h.next_block();
        }
        assert_eq!(h.read(REG_HDMA5), 0xFF);
        assert!(!h.hblank_active());
    }

    #[test]
    fn hblank_dma_counts_down_and_can_be_cancelled() {
        let mut h = programmed();
        assert_eq!(h.write(REG_HDMA5, 0x83), Start::HBlank);
        assert_eq!(
            h.read(REG_HDMA5),
            0x03,
            "active: bit 7 clear, 4 blocks left"
        );
        h.next_block();
        assert_eq!(h.read(REG_HDMA5), 0x02);
        assert_eq!(
            h.write(REG_HDMA5, 0x00),
            Start::Nothing,
            "bit 7 clear cancels"
        );
        assert_eq!(h.read(REG_HDMA5), 0x82, "stopped: bit 7 set, length kept");
        assert!(!h.hblank_active());
    }

    #[test]
    fn hblank_dma_finishes_with_ff() {
        let mut h = programmed();
        h.write(REG_HDMA5, 0x80); // one block
        h.next_block();
        assert_eq!(h.read(REG_HDMA5), 0xFF);
    }
}
