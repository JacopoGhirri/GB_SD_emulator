//! Battery save round trip at the mapper level: write cartridge RAM, take the save bytes, load
//! them into a brand-new cartridge/GameBoy and read the same data back. Phase 4 gate item.

use gb_core::{Cartridge, GameBoy, Model};

/// 32 KiB-per-bank-count ROM with just enough header for `Cartridge::from_bytes`.
/// `ram_code`: header byte 0x149 (3 = 32 KiB, 2 = 8 KiB).
fn rom(cart_type: u8, ram_code: u8) -> Vec<u8> {
    let mut rom = vec![0u8; 0x8000];
    rom[0x147] = cart_type;
    rom[0x148] = 0; // 32 KiB, two banks
    rom[0x149] = ram_code;
    let sum = rom[0x134..=0x14C]
        .iter()
        .fold(0u8, |a, &b| a.wrapping_sub(b).wrapping_sub(1));
    rom[0x14D] = sum;
    rom
}

/// Enable RAM, then write a recognisable pattern into every RAM bank.
fn fill(cart: &mut Cartridge, banks: u8, bank_select_addr: u16) {
    cart.write_rom(0x0000, 0x0A);
    for bank in 0..banks {
        cart.write_rom(bank_select_addr, bank);
        for off in [0u16, 1, 0x7F, 0x1FFF] {
            cart.write_ram(0xA000 + off, bank.wrapping_mul(37).wrapping_add(off as u8));
        }
    }
}

fn check(cart: &mut Cartridge, banks: u8, bank_select_addr: u16) {
    cart.write_rom(0x0000, 0x0A);
    for bank in 0..banks {
        cart.write_rom(bank_select_addr, bank);
        for off in [0u16, 1, 0x7F, 0x1FFF] {
            assert_eq!(
                cart.read_ram(0xA000 + off),
                bank.wrapping_mul(37).wrapping_add(off as u8),
                "bank {bank} offset {off:#x}"
            );
        }
    }
}

fn round_trip(cart_type: u8, banks: u8, select_addr: u16) {
    let bytes = rom(cart_type, 3);
    let mut a = Cartridge::from_bytes(bytes.clone()).expect("valid test ROM");
    fill(&mut a, banks, select_addr);
    let save = a.save_data();
    assert!(save.len() >= 0x8000, "RAM must be in the save");

    let mut b = Cartridge::from_bytes(bytes).expect("valid test ROM");
    b.load_save(&save);
    check(&mut b, banks, select_addr);
}

#[test]
fn mbc1_battery_ram_round_trips() {
    // MBC1 banks RAM through 0x4000-0x5FFF only in RAM-banking mode (0x6000 = 1).
    let bytes = rom(0x03, 3);
    let mut a = Cartridge::from_bytes(bytes.clone()).expect("valid");
    a.write_rom(0x6000, 1);
    fill(&mut a, 4, 0x4000);
    let save = a.save_data();
    let mut b = Cartridge::from_bytes(bytes).expect("valid");
    b.load_save(&save);
    b.write_rom(0x6000, 1);
    check(&mut b, 4, 0x4000);
}

#[test]
fn mbc3_battery_ram_round_trips() {
    round_trip(0x13, 4, 0x4000);
}

#[test]
fn mbc5_battery_ram_round_trips() {
    round_trip(0x1B, 4, 0x4000);
}

#[test]
fn mbc3_rtc_footer_and_timestamp_round_trip_through_gameboy() {
    let bytes = rom(0x10, 3);
    let mut gb = GameBoy::new(
        Cartridge::from_bytes(bytes.clone()).expect("valid"),
        Model::Dmg,
    );
    assert!(gb.cartridge().has_rtc());
    gb.cartridge_mut().set_save_timestamp(1_700_000_000);
    gb.cartridge_mut().advance_rtc_seconds(3725);
    let save = gb.save_ram();

    let mut gb2 = GameBoy::new(Cartridge::from_bytes(bytes).expect("valid"), Model::Dmg);
    gb2.load_save_ram(&save);
    assert_eq!(gb2.cartridge().save_timestamp(), Some(1_700_000_000));
    assert_eq!(
        gb2.save_ram(),
        save,
        "save bytes are stable across a reload"
    );
}

#[test]
fn cartridge_without_battery_has_empty_save() {
    let cart = Cartridge::from_bytes(rom(0x01, 0)).expect("valid");
    assert!(cart.save_data().is_empty());
}
