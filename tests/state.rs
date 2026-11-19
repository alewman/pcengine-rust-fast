// SPDX-License-Identifier: MIT
//! Save states (state.rs): a saved and loaded run continues as the run that was not
//! interrupted; the refusals; the snapshot pair. The reference's tests/test_state.py on its
//! own synthetic card, so nothing here needs a ROM.

use pcengine_rust::board::{Board, BoardOptions};
use pcengine_rust::cards::{PopulousCard, RomCard, Sf2Card, TennokoeCard};
use pcengine_rust::fake_devices::{LockstepCard, LockstepPort};
use pcengine_rust::machine::{Machine, PC_ENGINE, TURBOGRAFX_16};
use pcengine_rust::pads::{Pad2, Pad6};
use pcengine_rust::sockets::Card;
use pcengine_rust::state::{load_snapshot, load_state, save_snapshot, save_state, MAGIC};
use pcengine_rust::system::System;
use pcengine_rust::trace::{roundtrip, runahead, Setup};

fn system_with(card: Box<dyn Card>, machine: Machine) -> System {
    System::new(
        Board::new(BoardOptions {
            machine,
            card: Some(card),
            pad: Some(Box::new(Pad2::new())),
            ..BoardOptions::default()
        })
        .unwrap(),
    )
}

/// A card image that is only its reset vector (to $E000) and a fill.
fn image(size: usize) -> Vec<u8> {
    let mut data = vec![0xEA; size];
    data[size - 2] = 0x00;
    data[size - 1] = 0xE0;
    data
}

/// A card that does something: counts in work RAM and writes the count to the VCE's colour
/// table and a PSG register, for ever (so a state has RAM, a palette and sound to carry).
fn program() -> Vec<u8> {
    let code: &[u8] = &[
        0x78, // SEI
        0xA9, 0xFF, 0x53, 0x02, // LDA #$FF; TAM #$02: the hardware page at $2000
        0xA9, 0xF8, 0x53, 0x04, // LDA #$F8; TAM #$04: work RAM at $4000
        0xEE, 0x00, 0x40, // loop ($E009): INC $4000
        0xAD, 0x00, 0x40, // LDA $4000
        0x8D, 0x02, 0x24, // STA $2402 (VCE colour address, low)
        0x8D, 0x04, 0x24, // STA $2404 (VCE colour data, low)
        0x8D, 0x06, 0x28, // STA $2806 (PSG channel data)
        0x4C, 0x09, 0xE0, // JMP loop
    ];
    let mut data = image(0x2000);
    data[..code.len()].copy_from_slice(code);
    data
}

fn rom(data: &[u8]) -> Box<dyn Card> {
    Box::new(RomCard::new(data).unwrap())
}

#[test]
fn a_loaded_run_continues_bus_cycle_for_bus_cycle() {
    let setup = Setup::for_image(program());
    let what = roundtrip(&setup, 2, 4, 777).unwrap();
    assert!(
        what.contains("2 frames after the save equal, twice"),
        "{what}"
    );
}

#[test]
fn a_loaded_run_with_a_device_in_every_socket_continues_the_same() {
    for device in [
        "lockstep-card",
        "lockstep-port",
        "lockstep-path",
        "pad6",
        "multitap",
    ] {
        let mut setup = Setup::for_image(program());
        setup.device = device.to_string();
        setup.seed = Some(5);
        roundtrip(&setup, 2, 4, 333).unwrap_or_else(|error| panic!("{device}: {error}"));
    }
}

#[test]
fn a_snapshot_put_back_leaves_the_run_as_it_was() {
    let setup = Setup::for_image(program());
    runahead(&setup, 6, 2).unwrap();
    let mut system = setup.build().unwrap();
    system.run_frame();
    let snapshot = save_snapshot(&system);
    let before = save_state(&system);
    system.run_frame();
    assert_ne!(save_state(&system), before);
    load_snapshot(&mut system, &snapshot).unwrap();
    assert_eq!(save_state(&system), before);
    assert!(load_snapshot(&mut system, b"short").is_err());
}

#[test]
fn a_state_holds_no_rom_bytes() {
    let mut data = program();
    data[0x1000..0x1010].copy_from_slice(b"NOT-IN-THE-STATE");
    let mut system = system_with(rom(&data), PC_ENGINE);
    system.run_frame();
    let state = save_state(&system);
    assert_eq!(&state[..8], MAGIC);
    let mut plain = Vec::new();
    std::io::Read::read_to_end(
        &mut flate2::read::ZlibDecoder::new(&state[12..]),
        &mut plain,
    )
    .unwrap();
    assert!(!plain
        .windows(16)
        .any(|window| window == b"NOT-IN-THE-STATE"));
    // the card is in the header by name: its SHA-1 and length
    assert!(plain.windows(11).any(|window| window == b"length 8192"));
}

fn card_of<T: 'static>(system: &mut System) -> &mut T {
    let card = system.board_mut().slot.card.as_mut().unwrap().as_any_mut();
    card.downcast_mut::<T>().unwrap()
}

#[test]
fn the_mappers_registers_and_ram_come_back() {
    let sf2 = || -> Box<dyn Card> { Box::new(Sf2Card::new(&image(0x280000)).unwrap()) };
    let mut first = system_with(sf2(), PC_ENGINE);
    card_of::<Sf2Card>(&mut first).page = 2;
    let mut other = system_with(sf2(), PC_ENGINE);
    load_state(&mut other, &save_state(&first)).unwrap();
    assert_eq!(card_of::<Sf2Card>(&mut other).page, 2);

    let populous = || -> Box<dyn Card> { Box::new(PopulousCard::new(&image(0x80000)).unwrap()) };
    let mut first = system_with(populous(), PC_ENGINE);
    card_of::<PopulousCard>(&mut first).ram[0x1234] = 0x5A;
    let mut other = system_with(populous(), PC_ENGINE);
    load_state(&mut other, &save_state(&first)).unwrap();
    assert_eq!(card_of::<PopulousCard>(&mut other).ram[0x1234], 0x5A);

    let tennokoe = || -> Box<dyn Card> { Box::new(TennokoeCard::new(&image(0x2000)).unwrap()) };
    let mut first = system_with(tennokoe(), PC_ENGINE);
    card_of::<TennokoeCard>(&mut first).locked = false;
    card_of::<TennokoeCard>(&mut first).ram[7] = 0x33;
    let mut other = system_with(tennokoe(), PC_ENGINE);
    load_state(&mut other, &save_state(&first)).unwrap();
    assert!(!card_of::<TennokoeCard>(&mut other).locked);
    assert_eq!(card_of::<TennokoeCard>(&mut other).ram[7], 0x33);
}

#[test]
fn loading_keeps_the_wiring_and_the_switches() {
    let mut system = system_with(rom(&program()), PC_ENGINE);
    system.run_frame();
    let mut other = System::new(
        Board::new(BoardOptions {
            card: Some(rom(&program())),
            pad: Some(Box::new(Pad2::new())),
            timer_underflow_window_mclk: 1,
            psg: pcengine_rust::psg::PsgSwitches::variant("mame").unwrap(),
            ..BoardOptions::default()
        })
        .unwrap(),
    );
    other.board_mut().vce.rgb_table[5] = (1, 2, 3);
    load_state(&mut other, &save_state(&system)).unwrap();
    let board = other.board();
    assert_eq!(board.mclk, system.board().mclk);
    assert_eq!(other.frame(), system.frame());
    assert_eq!(other.cpu.capture_state(), system.cpu.capture_state());
    // what the reference keeps as class attributes is the system's own, not the state's
    assert_eq!(board.cpu_io.timer.underflow_window_mclk, 1);
    assert!(board.psg.switches.scale_table_quirk);
    assert_eq!(board.vce.rgb_table[5], (1, 2, 3));
    assert_eq!(board.vce.rgb_table.len(), 512);
    // the page table still decodes
    assert_eq!(other.board_mut().page_read(0x001FFF, 0), 0xE0);
}

fn refusal(system: &mut System, state: &[u8]) -> String {
    let before = save_state(system);
    let error = load_state(system, state).unwrap_err();
    assert_eq!(
        save_state(system),
        before,
        "a refused state changed the system"
    );
    error
}

#[test]
fn a_state_of_something_else_is_refused() {
    let mut system = system_with(rom(&program()), PC_ENGINE);
    system.run_frame();
    let state = save_state(&system);

    assert!(refusal(&mut system, b"not zlib").contains("not a pcengine-rust state"));
    let mut garbage = state[..12].to_vec();
    garbage.extend_from_slice(b"not zlib either");
    assert!(refusal(&mut system, &garbage).contains("not a save state"));
    let mut other_version = state.clone();
    other_version[8] = 0;
    assert!(refusal(&mut system, &other_version).contains("version 0"));

    let mut tg16 = system_with(rom(&program()), TURBOGRAFX_16);
    assert!(refusal(&mut tg16, &state).contains("of the PC Engine, not the TurboGrafx-16"));

    let mut another_card = system_with(rom(&image(0x2000)), PC_ENGINE);
    assert!(refusal(&mut another_card, &state).contains("another ROM image"));
    // the same bytes behind another mapper are another card
    let mut another_mapper =
        system_with(Box::new(TennokoeCard::new(&program()).unwrap()), PC_ENGINE);
    assert!(refusal(&mut another_mapper, &state).contains("another ROM image"));
    let mut empty_slot = System::new(Board::new(BoardOptions::default()).unwrap());
    assert!(refusal(&mut empty_slot, &state).contains("another ROM image"));
}

#[test]
fn a_state_of_a_machine_built_another_way_is_refused() {
    let build = |port: bool, pad6: bool, vdcs: usize, card: bool| {
        let card: Box<dyn Card> = if card {
            Box::new(LockstepCard::new(&program()).unwrap())
        } else {
            rom(&program())
        };
        System::new(
            Board::new(BoardOptions {
                machine: Machine {
                    vdc_count: vdcs,
                    ..PC_ENGINE
                },
                card: Some(card),
                port: if port {
                    Some(Box::new(LockstepPort::new()))
                } else {
                    None
                },
                pad: Some(if pad6 {
                    Box::new(Pad6::new())
                } else {
                    Box::new(Pad2::new())
                }),
                ..BoardOptions::default()
            })
            .unwrap(),
        )
    };
    let state = save_state(&build(false, false, 1, false));
    let with_port = save_state(&build(true, false, 1, false));
    // an expansion device the state has not, and the other way
    assert!(refusal(&mut build(true, false, 1, false), &state).contains("expansion device"));
    assert!(refusal(&mut build(false, false, 1, false), &with_port).contains("expansion device"));
    // another pad in the port
    assert!(refusal(&mut build(false, true, 1, false), &state).contains("built another way"));
    // a second VDC under the same machine name
    assert!(refusal(&mut build(false, false, 2, false), &state).contains("built another way"));
    // the same image in a card of another kind
    assert!(refusal(&mut build(false, false, 1, true), &state).contains("another ROM image"));
    // and each loads into its own kind
    load_state(&mut build(true, false, 1, false), &with_port).unwrap();
}

/// A state saved by pcengine-rust 0.1.0 (format version 1, before huc6280-rust 0.2.0 gave the
/// CPU's state `polled_irq_lines`) from `program()` after one frame: refused cleanly, not
/// converted (state.rs).
#[test]
fn a_state_of_the_previous_format_is_refused() {
    let old = include_bytes!("data/state-v1.pcestate");
    let mut system = system_with(rom(&program()), PC_ENGINE);
    system.run_frame();
    let error = refusal(&mut system, old);
    assert!(
        error.contains("version 1, not 3"),
        "unexpected refusal: {error}"
    );
}

/// A state saved by pcengine-rust 0.2.0 (format version 2, the VDC's withdrawn line machine)
/// from `program()` after one frame: refused cleanly, not converted (state.rs).
#[test]
fn a_state_of_the_withdrawn_line_machine_is_refused() {
    let old = include_bytes!("data/state-v2.pcestate");
    let mut system = system_with(rom(&program()), PC_ENGINE);
    system.run_frame();
    let error = refusal(&mut system, old);
    assert!(
        error.contains("version 2, not 3"),
        "unexpected refusal: {error}"
    );
}

/// The VDCs' wiring to the board's wait (issue 0004) is not state: a loaded state keeps it.
#[test]
fn loading_keeps_the_vdcs_wired_to_the_boards_wait() {
    let mut system = system_with(rom(&program()), PC_ENGINE);
    system.run_frame();
    let state = save_state(&system);
    let mut other = system_with(rom(&program()), PC_ENGINE);
    load_state(&mut other, &state).unwrap();
    assert!(other.board().vdcs.iter().all(|vdc| vdc.serve));
    let snapshot = save_snapshot(&system);
    load_snapshot(&mut other, &snapshot).unwrap();
    assert!(other.board().vdcs.iter().all(|vdc| vdc.serve));
}

/// The CPU's `polled_irq_lines` (huc6280-python 0.4.0, cleared at each boundary since 6ce348e)
/// is in a state: found at a boundary where it is set (a timer interrupt line that rises and
/// is acknowledged in a loop with interrupts masked, so it changes in some instruction's last
/// cycle sooner or later), saved and loaded, the CPU's state is equal field for field.
#[test]
fn a_state_carries_the_lines_the_last_poll_saw() {
    let code: &[u8] = &[
        0x78, // SEI
        0xA9, 0xFF, 0x53, 0x02, // LDA #$FF; TAM #$02: the hardware page at $2000
        0xA9, 0x00, 0x8D, 0x00, 0x2C, // LDA #0; STA $2C00: timer latch 0
        0xA9, 0x01, 0x8D, 0x01, 0x2C, // LDA #1; STA $2C01: timer on
        0x8D, 0x03, 0x34, // loop ($E00F): STA $3403 (acknowledge the timer)
        0xEA, 0xEA, // NOP; NOP
        0x4C, 0x0F, 0xE0, // JMP loop
    ];
    let mut data = image(0x2000);
    data[..code.len()].copy_from_slice(code);
    let mut system = system_with(rom(&data), PC_ENGINE);
    let mut found = false;
    for _ in 0..2_000_000 {
        system.step();
        if system.cpu.capture_state().polled_irq_lines.is_some() {
            found = true;
            break;
        }
    }
    assert!(found, "no boundary with the poll's lines set");
    let state = save_state(&system);
    let mut other = system_with(rom(&data), PC_ENGINE);
    load_state(&mut other, &state).unwrap();
    assert_eq!(system.cpu.capture_state(), other.cpu.capture_state());
}
