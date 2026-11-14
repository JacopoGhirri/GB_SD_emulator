//! MBC3 with optional real-time clock (Pan Docs "MBC3").
//!
//! * 0000-1FFF: enables both cartridge RAM and the RTC registers (0x0A).
//! * 2000-3FFF: ROM bank, 7 bits, 0 reads as 1.
//! * 4000-5FFF: 0x00-0x03 selects a RAM bank, 0x08-0x0C selects an RTC register.
//! * 6000-7FFF: writing 0x00 then 0x01 copies the running clock into the latched registers
//!   that the RTC reads return.
//!
//! The RTC counts *emulated* time (1 second = 1,048,576 M-cycles), so runs are deterministic.
//! The core never reads a wall clock. A frontend that wants the clock to keep running while the
//! emulator is closed stamps the save with the current Unix time (`set_save_timestamp`) and, on
//! the next start, advances the clock by the elapsed seconds (`advance_rtc_seconds`).
//!
//! ## Save format
//! `save_data()` is the cartridge RAM followed, for MBC3+TIMER cartridges, by the 48-byte RTC
//! footer used by BGB, VBA-M and other emulators (https://bgb.bircd.org/rtcsave.html), so saves
//! can move between emulators. All fields are little-endian u32:
//!   0..20  live registers: seconds, minutes, hours, days low, days high/flags
//!   20..40 latched registers, same order
//!   40..48 Unix timestamp (u64) of when the save was written
//! Loading also accepts the older 44-byte footer (32-bit timestamp) and no footer at all. The
//! timestamp is whatever the frontend set (0 by default, i.e. "unknown"); loading just remembers it.

use super::{ram_offset, Mapper, Ram};

const CYCLES_PER_SECOND: u32 = 1_048_576;
const RTC_FOOTER_LEN: usize = 48;
const RTC_FOOTER_LEN_OLD: usize = 44;

/// Days-high register bits.
const DAY_HIGH_BIT8: u8 = 0x01;
const DAY_HIGH_HALT: u8 = 0x40;
const DAY_HIGH_CARRY: u8 = 0x80;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Rtc {
    seconds: u8,
    minutes: u8,
    hours: u8,
    /// 9-bit day counter.
    days: u16,
    halted: bool,
    day_carry: bool,
    /// M-cycles accumulated towards the next second.
    sub_cycles: u32,
    latched: [u8; 5],
    /// Last value written to the latch register; a 0 -> 1 sequence latches.
    latch_armed: bool,
    /// Unix time stored in the save footer (0 = unknown). Only the frontend gives it meaning.
    timestamp: u64,
}

impl Rtc {
    fn registers(&self) -> [u8; 5] {
        let mut high = ((self.days >> 8) & 1) as u8;
        if self.halted {
            high |= DAY_HIGH_HALT;
        }
        if self.day_carry {
            high |= DAY_HIGH_CARRY;
        }
        [
            self.seconds,
            self.minutes,
            self.hours,
            self.days as u8,
            high,
        ]
    }

    fn set_register(&mut self, index: u8, value: u8) {
        match index {
            0 => {
                self.seconds = value & 0x3F;
                self.sub_cycles = 0; // writing seconds restarts the sub-second divider
            }
            1 => self.minutes = value & 0x3F,
            2 => self.hours = value & 0x1F,
            3 => self.days = (self.days & 0x100) | u16::from(value),
            _ => {
                self.days = (self.days & 0xFF) | (u16::from(value & DAY_HIGH_BIT8) << 8);
                self.halted = value & DAY_HIGH_HALT != 0;
                self.day_carry = value & DAY_HIGH_CARRY != 0;
            }
        }
    }

    fn tick(&mut self) {
        if self.halted {
            return;
        }
        self.sub_cycles += 1;
        if self.sub_cycles >= CYCLES_PER_SECOND {
            self.sub_cycles = 0;
            self.advance_second();
        }
    }

    fn advance_second(&mut self) {
        self.seconds += 1;
        if self.seconds < 60 {
            return;
        }
        self.seconds = 0;
        self.minutes += 1;
        if self.minutes < 60 {
            return;
        }
        self.minutes = 0;
        self.hours += 1;
        if self.hours < 24 {
            return;
        }
        self.hours = 0;
        self.days += 1;
        if self.days >= 512 {
            // The day counter wraps and the carry flag stays set until software clears it.
            self.days = 0;
            self.day_carry = true;
        }
    }

    /// Move the running clock forward by `secs` real seconds (a halted clock does not move).
    /// The latched registers are left alone, as on hardware.
    fn advance_seconds(&mut self, secs: u64) {
        if self.halted {
            return;
        }
        let day_secs = 86_400u64;
        let now = self.seconds as u64
            + 60 * self.minutes as u64
            + 3600 * self.hours as u64
            + day_secs * self.days as u64;
        let total = now + secs;
        let days = total / day_secs;
        if days >= 512 {
            self.day_carry = true;
        }
        let rem = total % day_secs;
        self.days = (days % 512) as u16;
        self.hours = (rem / 3600) as u8;
        self.minutes = (rem % 3600 / 60) as u8;
        self.seconds = (rem % 60) as u8;
    }

    fn write_latch(&mut self, value: u8) {
        if value == 0 {
            self.latch_armed = true;
        } else {
            if value == 1 && self.latch_armed {
                self.latched = self.registers();
            }
            self.latch_armed = false;
        }
    }

    fn footer(&self) -> [u8; RTC_FOOTER_LEN] {
        let mut out = [0u8; RTC_FOOTER_LEN];
        for (i, v) in self.registers().iter().enumerate() {
            out[i * 4] = *v;
        }
        for (i, v) in self.latched.iter().enumerate() {
            out[20 + i * 4] = *v;
        }
        out[40..48].copy_from_slice(&self.timestamp.to_le_bytes());
        out
    }

    fn load_footer(&mut self, footer: &[u8]) {
        let reg = |i: usize| footer[i * 4];
        for i in 0..5 {
            self.set_register(i as u8, reg(i));
            self.latched[i] = reg(5 + i);
        }
        self.sub_cycles = 0;
        self.timestamp = if footer.len() == RTC_FOOTER_LEN {
            u64::from_le_bytes(footer[40..48].try_into().unwrap_or([0; 8]))
        } else {
            u32::from_le_bytes(footer[40..44].try_into().unwrap_or([0; 4])) as u64
        };
    }
}

#[derive(Debug)]
pub struct Mbc3 {
    rom_banks: usize,
    ram: Ram,
    battery: bool,
    /// `Some` on MBC3+TIMER cartridges.
    rtc: Option<Rtc>,
    ram_and_rtc_enabled: bool,
    rom_bank: u8,
    /// 0x00-0x03: RAM bank; 0x08-0x0C: RTC register.
    select: u8,
}

impl Mbc3 {
    pub fn new(rom_banks: usize, ram: Ram, battery: bool, has_rtc: bool) -> Mbc3 {
        Mbc3 {
            rom_banks,
            ram,
            battery,
            rtc: has_rtc.then(Rtc::default),
            ram_and_rtc_enabled: false,
            rom_bank: 1,
            select: 0,
        }
    }
}

impl Mapper for Mbc3 {
    fn read_rom(&self, rom: &[u8], addr: u16) -> u8 {
        let bank = if addr < 0x4000 {
            0
        } else {
            self.rom_bank as usize % self.rom_banks
        };
        rom.get(bank * super::ROM_BANK_SIZE + (addr as usize & 0x3FFF))
            .copied()
            .unwrap_or(0xFF)
    }

    fn write_rom(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.ram_and_rtc_enabled = value & 0x0F == 0x0A,
            0x2000..=0x3FFF => {
                let v = value & 0x7F;
                self.rom_bank = if v == 0 { 1 } else { v };
            }
            0x4000..=0x5FFF => self.select = value,
            _ => {
                if let Some(rtc) = self.rtc.as_mut() {
                    rtc.write_latch(value);
                }
            }
        }
    }

    fn read_ram(&self, addr: u16) -> u8 {
        if !self.ram_and_rtc_enabled {
            return 0xFF;
        }
        match self.select {
            0x00..=0x03 => self.ram.read(ram_offset(self.select as usize, addr)),
            0x08..=0x0C => match &self.rtc {
                Some(rtc) => rtc.latched[(self.select - 0x08) as usize],
                None => 0xFF,
            },
            _ => 0xFF,
        }
    }

    fn write_ram(&mut self, addr: u16, value: u8) {
        if !self.ram_and_rtc_enabled {
            return;
        }
        match self.select {
            0x00..=0x03 => self
                .ram
                .write(ram_offset(self.select as usize, addr), value),
            0x08..=0x0C => {
                if let Some(rtc) = self.rtc.as_mut() {
                    rtc.set_register(self.select - 0x08, value);
                }
            }
            _ => {}
        }
    }

    fn tick(&mut self) {
        if let Some(rtc) = self.rtc.as_mut() {
            rtc.tick();
        }
    }

    fn set_save_timestamp(&mut self, unix_secs: u64) {
        if let Some(rtc) = self.rtc.as_mut() {
            rtc.timestamp = unix_secs;
        }
    }

    fn save_timestamp(&self) -> Option<u64> {
        self.rtc.as_ref().map(|r| r.timestamp)
    }

    fn advance_rtc_seconds(&mut self, secs: u64) {
        if let Some(rtc) = self.rtc.as_mut() {
            rtc.advance_seconds(secs);
        }
    }

    fn save_data(&self) -> Vec<u8> {
        if !self.battery {
            return Vec::new();
        }
        let mut out = self.ram.bytes().to_vec();
        if let Some(rtc) = &self.rtc {
            out.extend_from_slice(&rtc.footer());
        }
        out
    }

    fn load_save(&mut self, data: &[u8]) {
        if !self.battery {
            return;
        }
        let ram_len = self.ram.len();
        self.ram.load(data);
        let Some(rtc) = self.rtc.as_mut() else {
            return;
        };
        // Footer sits right after the RAM; accept the 48-byte and the older 44-byte layout.
        if let Some(footer) = data.get(ram_len..) {
            if footer.len() == RTC_FOOTER_LEN || footer.len() == RTC_FOOTER_LEN_OLD {
                rtc.load_footer(footer);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn wall_clock_advance_carries_and_respects_halt() {
        let mut m = mbc(0x2000, true);
        m.set_save_timestamp(1234);
        m.advance_rtc_seconds(90_061); // 1 d 1 h 1 min 1 s
        let r = m.rtc.as_ref().unwrap();
        assert_eq!((r.days, r.hours, r.minutes, r.seconds), (1, 1, 1, 1));
        // 511 more days pushes the day counter over 511: wraps and sets carry.
        m.advance_rtc_seconds(511 * 86_400);
        let r = m.rtc.as_ref().unwrap();
        assert_eq!(r.days, 0);
        assert!(r.day_carry);
        // Timestamp survives a save/load round trip.
        let saved = m.save_data();
        let mut n = mbc(0x2000, true);
        n.load_save(&saved);
        assert_eq!(n.save_timestamp(), Some(1234));
        // A halted clock does not move.
        let mut h = mbc(0x2000, true);
        h.rtc.as_mut().unwrap().halted = true;
        h.advance_rtc_seconds(100);
        assert_eq!(h.rtc.as_ref().unwrap().seconds, 0);
    }
    use super::*;

    fn mbc(ram: usize, rtc: bool) -> Mbc3 {
        let mut m = Mbc3::new(128, Ram::new(ram), true, rtc);
        m.write_rom(0x0000, 0x0A);
        m
    }

    #[test]
    fn rom_bank_is_seven_bits_and_zero_reads_as_one() {
        let mut rom = vec![0u8; 128 * super::super::ROM_BANK_SIZE];
        for b in 0..128 {
            rom[b * super::super::ROM_BANK_SIZE] = b as u8;
        }
        let mut m = mbc(0, false);
        m.write_rom(0x2000, 0);
        assert_eq!(m.read_rom(&rom, 0x4000), 1);
        m.write_rom(0x2000, 0x7F);
        assert_eq!(m.read_rom(&rom, 0x4000), 0x7F);
        m.write_rom(0x2000, 0xFF); // bit 7 ignored
        assert_eq!(m.read_rom(&rom, 0x4000), 0x7F);
    }

    #[test]
    fn ram_banks_and_enable() {
        let mut m = Mbc3::new(2, Ram::new(32 * 1024), true, false);
        m.write_ram(0xA000, 1);
        assert_eq!(m.read_ram(0xA000), 0xFF, "disabled");
        m.write_rom(0x0000, 0x0A);
        m.write_rom(0x4000, 3);
        m.write_ram(0xA000, 0x42);
        m.write_rom(0x4000, 0);
        assert_eq!(m.read_ram(0xA000), 0);
        m.write_rom(0x4000, 3);
        assert_eq!(m.read_ram(0xA000), 0x42);
    }

    fn run_seconds(m: &mut Mbc3, seconds: u32) {
        for _ in 0..seconds * CYCLES_PER_SECOND {
            m.tick();
        }
    }

    fn latch(m: &mut Mbc3) {
        m.write_rom(0x6000, 0);
        m.write_rom(0x6000, 1);
    }

    fn read_reg(m: &mut Mbc3, reg: u8) -> u8 {
        m.write_rom(0x4000, 0x08 + reg);
        m.read_ram(0xA000)
    }

    #[test]
    fn rtc_reads_only_change_on_latch() {
        let mut m = mbc(0, true);
        run_seconds(&mut m, 5);
        assert_eq!(read_reg(&mut m, 0), 0, "not latched yet");
        latch(&mut m);
        assert_eq!(read_reg(&mut m, 0), 5);
        run_seconds(&mut m, 2);
        assert_eq!(read_reg(&mut m, 0), 5, "latched value is frozen");
        m.write_rom(0x6000, 1); // 1 without a preceding 0 does not latch
        assert_eq!(read_reg(&mut m, 0), 5);
        latch(&mut m);
        assert_eq!(read_reg(&mut m, 0), 7);
    }

    #[test]
    fn rtc_carries_into_minutes_hours_days() {
        let mut m = mbc(0, true);
        m.write_rom(0x4000, 0x08);
        m.write_ram(0xA000, 59);
        m.write_rom(0x4000, 0x09);
        m.write_ram(0xA000, 59);
        m.write_rom(0x4000, 0x0A);
        m.write_ram(0xA000, 23);
        run_seconds(&mut m, 1);
        latch(&mut m);
        assert_eq!(
            [
                read_reg(&mut m, 0),
                read_reg(&mut m, 1),
                read_reg(&mut m, 2)
            ],
            [0, 0, 0]
        );
        assert_eq!(read_reg(&mut m, 3), 1, "day counter");
    }

    #[test]
    fn day_counter_overflow_sets_carry_and_halt_stops_time() {
        let mut m = mbc(0, true);
        m.write_rom(0x4000, 0x0B);
        m.write_ram(0xA000, 0xFF);
        m.write_rom(0x4000, 0x0C);
        m.write_ram(0xA000, DAY_HIGH_BIT8); // day 511
        m.write_rom(0x4000, 0x0A);
        m.write_ram(0xA000, 23);
        m.write_rom(0x4000, 0x09);
        m.write_ram(0xA000, 59);
        m.write_rom(0x4000, 0x08);
        m.write_ram(0xA000, 59);
        run_seconds(&mut m, 1);
        latch(&mut m);
        assert_eq!(
            read_reg(&mut m, 4),
            DAY_HIGH_CARRY,
            "wrapped to day 0 with carry"
        );
        assert_eq!(read_reg(&mut m, 3), 0);
        m.write_rom(0x4000, 0x0C);
        m.write_ram(0xA000, DAY_HIGH_HALT | DAY_HIGH_CARRY);
        run_seconds(&mut m, 3);
        latch(&mut m);
        assert_eq!(read_reg(&mut m, 0), 0, "halted clock does not advance");
    }

    #[test]
    fn save_has_bgb_compatible_footer_and_round_trips() {
        let mut m = mbc(8192, true);
        m.write_rom(0x4000, 0);
        m.write_ram(0xA000, 0x99);
        m.write_rom(0x4000, 0x0A);
        m.write_ram(0xA000, 7); // hours
        run_seconds(&mut m, 3);
        latch(&mut m);
        let saved = m.save_data();
        assert_eq!(saved.len(), 8192 + 48);
        assert_eq!(saved[8192 + 2 * 4], 7, "live hours at dword 2");
        assert_eq!(saved[8192], 3, "live seconds at dword 0");
        assert_eq!(saved[8192 + 20], 3, "latched seconds at dword 5");

        let mut fresh = mbc(8192, true);
        fresh.load_save(&saved);
        fresh.write_rom(0x4000, 0);
        assert_eq!(fresh.read_ram(0xA000), 0x99);
        assert_eq!(read_reg(&mut fresh, 0), 3, "latched seconds restored");
        latch(&mut fresh);
        assert_eq!(read_reg(&mut fresh, 2), 7);
    }

    #[test]
    fn old_44_byte_footer_and_missing_footer_load() {
        let mut m = mbc(8192, true);
        let mut data = vec![0u8; 8192 + 44];
        data[8192 + 4] = 9; // minutes
        m.load_save(&data);
        latch(&mut m);
        assert_eq!(read_reg(&mut m, 1), 9);

        let mut m = mbc(8192, true);
        m.load_save(&vec![0u8; 8192]); // RAM only
        m.load_save(&[1, 2, 3]); // truncated: must not panic
    }

    #[test]
    fn cartridge_without_timer_has_no_footer() {
        let m = Mbc3::new(2, Ram::new(8192), true, false);
        assert_eq!(m.save_data().len(), 8192);
    }
}
