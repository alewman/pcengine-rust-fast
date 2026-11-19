// SPDX-License-Identifier: MIT
//! SF2, Populous, Tennokoe and the card factory on synthetic images (the reference's
//! tests/test_card_mappers.py; docs/cartridge.md: emulator tier for all three).

use pcengine_rust::cards::{bank_map, make_card, RomCard, Sf2Card, TennokoeCard, BANK};
use pcengine_rust::sockets::{Card, DetachedPins, ResetKind};

/// An image whose every 8 KiB bank holds its own number in every byte (mod 251).
fn banks(n: usize) -> Vec<u8> {
    (0..n).flat_map(|i| vec![(i % 251) as u8; BANK]).collect()
}

fn read(card: &mut dyn Card, bank: u32, offset: u32) -> u8 {
    card.read(
        (bank << 13) + offset,
        0,
        &mut DetachedPins::default().pins(),
    )
}

fn write(card: &mut dyn Card, address: u32, value: u8) {
    card.write(address, value, 0, &mut DetachedPins::default().pins());
}

#[test]
fn sf2_first_512k_fixed_and_the_upper_window_follows_the_latch() {
    let mut card = Sf2Card::new(&banks(320)).unwrap(); // 2.5 MiB = 320 banks
    assert_eq!([read(&mut card, 0, 0), read(&mut card, 0x3F, 0)], [0, 63]);
    // page 0 = ROM $80000-$FFFFF
    assert_eq!(
        [read(&mut card, 0x40, 0), read(&mut card, 0x7F, 0)],
        [64, 127]
    );
    for page in 1..=3u32 {
        write(&mut card, 0x1FF0 + page, 0); // bank $00, $1FF0-$1FF3
        assert_eq!(read(&mut card, 0x40, 0) as u32, (64 + 64 * page) % 251);
        assert_eq!(read(&mut card, 0x7F, 0) as u32, (127 + 64 * page) % 251);
    }
    assert_eq!(read(&mut card, 0x3F, 0), 63); // the fixed half never moves
}

#[test]
fn sf2_latch_decodes_1ff0_to_1fff_and_masks_by_the_page_count() {
    let mut card = Sf2Card::new(&banks(320)).unwrap();
    write(&mut card, 0x1FF7, 0); // 7 & 3 = 3 [emu: Mednafen masks; MAME decodes only $1FF0-$1FF3]
    assert_eq!(card.page, 3);
    write(&mut card, 0x1FEF, 0); // outside the latch
    write(&mut card, 0x2FF0, 0); // another bank-0 address: not the latch
    assert_eq!(card.page, 3);
    card.reset(ResetKind::PowerOn, &mut DetachedPins::default().pins());
    assert_eq!(card.page, 0);
}

#[test]
fn sf2_extended_images_have_more_pages_and_need_a_power_of_two() {
    // 512 KiB + 8.x pages of 512 KiB: rounded up to 16 pages
    let mut card = Sf2Card::new(&banks(64 * 9 + 8)).unwrap();
    assert_eq!(card.page_mask, 15);
    write(&mut card, 0x1FFF, 0);
    assert_eq!(card.page, 15);
    assert_eq!(read(&mut card, 0x40, 0), 0); // past the image: zero padding
}

#[test]
fn sf2_refuses_a_small_image() {
    assert!(Sf2Card::new(&banks(64)).is_err());
}

#[test]
fn populous_ram_is_four_unmirrored_banks_at_40_43_and_the_rest_is_rom() {
    let mut data = banks(32);
    data[0x1F26..0x1F26 + 8].copy_from_slice(b"POPULOUS");
    let mut card = make_card(&data).unwrap();
    assert!(card.state_identity().starts_with("PopulousCard"));
    let card = card.as_mut();
    assert_eq!(read(card, 0x40, 0), 0xFF); // Mednafen's power-up fill
    write(card, (0x40 << 13) + 5, 0xAA);
    write(card, (0x43 << 13) + 5, 0xBB);
    assert_eq!(read(card, 0x40, 5), 0xAA);
    assert_eq!(read(card, 0x43, 5), 0xBB);
    assert_eq!(read(card, 0x41, 5), 0xFF); // a different bank: not a mirror
    assert_eq!(read(card, 0x44, 0), 4); // beyond the RAM: the ROM mirror (32 banks)
    write(card, 0x10, 0x99); // ROM ignores writes
    assert_eq!(read(card, 0, 0x10), 0);
}

#[test]
fn tennokoe_bank_ram_is_locked_until_a_nonzero_write_to_f0000() {
    let mut card = TennokoeCard::new(&banks(64)).unwrap();
    assert_eq!(read(&mut card, 0x40, 3), 0xFF);
    write(&mut card, 0x80000 + 3, 0x11); // locked: ignored
    write(&mut card, 0xF0000, 1); // unlock [emu: MAME, "a complete guess"]
    write(&mut card, 0x80000 + 3, 0x11);
    assert_eq!(read(&mut card, 0x40, 3), 0x11);
    assert_eq!(read(&mut card, 0x47, 3), 0x11); // mirrored every 8 KiB through $8FFFF
    assert_eq!(read(&mut card, 0x20, 0), 32); // the ROM below is untouched
    assert_eq!(read(&mut card, 0x50, 0), 0xFF); // nothing decoded above it
    write(&mut card, 0xFFFFF, 0); // lock again
    assert_eq!(read(&mut card, 0x40, 3), 0xFF);
}

#[test]
fn the_factory_picks_plain_for_everything_else_and_strips_a_copier_header() {
    assert!(make_card(&banks(48))
        .unwrap()
        .state_identity()
        .starts_with("RomCard"));
    let mut headered = vec![0u8; 512];
    headered.extend(banks(32));
    let mut card = make_card(&headered).unwrap();
    assert!(card.state_identity().starts_with("RomCard"));
    assert_eq!(read(card.as_mut(), 5, 0), 5);
    assert!(make_card(&banks(320))
        .unwrap()
        .state_identity()
        .starts_with("Sf2Card"));
}

/// [emu: Mednafen, observed: the image repeats modulo its length in pages] the odd sizes of the library (72 KiB .. 872 KiB).
#[test]
fn odd_sizes_repeat_modulo_the_image_u18() {
    for count in [9usize, 15, 91, 96, 98, 109] {
        let mut card = RomCard::new(&banks(count)).unwrap();
        for bank in 0..128u32 {
            assert_eq!(
                read(&mut card, bank, 0) as usize,
                bank as usize % count % 251
            );
        }
    }
}

/// [emu: MAME `0 1 0 1 2 2 2 2`; Mednafen] 384 KiB, and 512 KiB with the upper half twice.
#[test]
fn the_384k_and_512k_layouts() {
    let map = bank_map(48);
    assert_eq!((map[0], map[31], map[32], map[63]), (0, 31, 0, 31));
    assert_eq!((map[64], map[79], map[80], map[127]), (32, 47, 32, 47));
    let map = bank_map(64);
    assert_eq!(
        (map[63], map[64], map[95], map[96], map[127]),
        (63, 32, 63, 32, 63)
    );
}

#[test]
fn an_empty_image_is_not_a_card_and_a_ragged_one_is_padded() {
    assert!(RomCard::new(&[]).is_err());
    let card = RomCard::new(&[0xEA; 0x2001]).unwrap();
    assert_eq!(card.data.len(), 0x4000);
    assert_eq!(card.data[0x2001], 0);
}
