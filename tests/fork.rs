//! The speed fork's own shortcuts (not in the reference or the base board): the event horizon
//! may skip `run_to` only where it would have found nothing to do, so the board's state after
//! every instruction is the state the base board's eager catch-up leaves.

use huc6280::{Bus, Pins};
use pcengine_rust::board::{Board, BoardOptions};
use pcengine_rust::cards::RomCard;
use pcengine_rust::pads::Pad2;
use pcengine_rust::sockets::{BoardPins, DeviceState, PageHandler, PortDevice, ResetKind};
use pcengine_rust::system::System;

/// A card that sets up a display, starts the timer and a VRAM-to-VRAM DMA (which the display
/// period pauses), waits, then writes VRAM words for ever (each waits for a CPU slot of the
/// line).
fn program() -> Vec<u8> {
    let code: &[u8] = &[
        0x78, // SEI
        0xA9, 0xFF, 0x53, 0x02, // LDA #$FF; TAM #$02: the hardware page at $2000
        0x03, 0x0B, 0x13, 0x1F, 0x23, 0x00, // ST0 #$0B; ST1 #$1F; ST2 #$00: HDR, 256 dots
        0x03, 0x0C, 0x13, 0x02, 0x23, 0x0F, // VPR: VSW 2, VDS 15
        0x03, 0x0D, 0x13, 0xEF, 0x23, 0x00, // VDW: 240 lines
        0x03, 0x05, 0x13, 0xC0, 0x23, 0x00, // CR: background and sprites on
        0xA9, 0x03, 0x8D, 0x00, 0x2C, // LDA #$03; STA $2C00: the timer's latch
        0xA9, 0x01, 0x8D, 0x01, 0x2C, // LDA #$01; STA $2C01: the timer runs
        0x03, 0x10, 0x13, 0x00, 0x23, 0x10, // SOUR $1000
        0x03, 0x11, 0x13, 0x00, 0x23, 0x20, // DESR $2000
        0x03, 0x12, 0x13, 0x00, 0x23, 0x40, // LENR $4000: the transfer starts
        0xA2, 0x20, 0xA0, 0x00, // LDX #$20; LDY #$00
        0x88, 0xD0, 0xFD, 0xCA, 0xD0, 0xFA, // wait: DEY; BNE wait; DEX; BNE wait (1.6 frames)
        0x03, 0x00, 0x13, 0x00, 0x23, 0x30, // MAWR $3000
        0x03, 0x02, // ST0 #$02: the data register (the first write ends the transfer)
        0x13, 0xAA, 0x23, 0x55, // loop ($E04B): ST1 #$AA; ST2 #$55: a word for VRAM
        0xEA, 0xEA, 0xEA, // NOP x 3
        0x4C, 0x4B, 0xE0, // JMP loop
    ];
    assert_eq!(code.len(), 0x4B + 10);
    let mut data = vec![0xEA; 0x2000];
    data[..code.len()].copy_from_slice(code);
    data[0x1FFE] = 0x00;
    data[0x1FFF] = 0xE0;
    data
}

fn system() -> System {
    System::new(
        Board::new(BoardOptions {
            card: Some(Box::new(RomCard::new(&program()).unwrap())),
            pad: Some(Box::new(Pad2::new())),
            ..BoardOptions::default()
        })
        .unwrap(),
    )
}

#[test]
fn every_device_is_caught_up_after_every_instruction() {
    let mut system = system();
    let (mut dma_ran, mut dma_waited, mut ticks, mut words) = (0, 0, 0, 0);
    for step in 0..40_000 {
        system.cpu.step();
        let board = &system.cpu.bus;
        let vdc = &board.vdcs[0];
        let mut eager = vdc.clone();
        eager.catch_up(board.mclk);
        assert!(
            eager == *vdc,
            "step {step}: the VDC is behind the clock {}",
            board.mclk
        );
        let display = vdc.vphase() == pcengine_rust::vdc::VPhase::Vdw;
        if vdc.dma.running {
            dma_ran += (vdc.regs.value[0x12] != 0x4000) as u32;
            dma_waited += display as u32;
        } else {
            // with no transfer the horizon is a time, not "every cycle"
            ticks += (board.cpu_io.timer.count != 3) as u32;
            words += (display && vdc.vram[0x3000..0x3800].contains(&0x55AA)) as u32;
        }
        let mut eager = board.cpu_io.timer;
        eager.catch_up(board.mclk);
        assert!(
            eager == board.cpu_io.timer,
            "step {step}: the timer is behind the clock {}",
            board.mclk
        );
    }
    // the run did reach what the horizon has rules for
    assert!(
        dma_ran > 0 && dma_waited > 0 && ticks > 0 && words > 0,
        "{dma_ran} {dma_waited} {ticks} {words}"
    );
}

#[test]
fn a_hold_from_outside_stretches_the_next_ram_access() {
    let work_ram = 0xF8 << 13;
    let mut pins = Pins::default();
    let mut plain = system();
    plain.run_frame();
    let board = &mut plain.cpu.bus;
    let before = board.mclk;
    board.read(work_ram, &mut pins);
    let cycle = board.mclk - before;

    let mut held = system();
    held.run_frame();
    held.board_mut().hold(7);
    let board = &mut held.cpu.bus;
    let before = board.mclk;
    board.read(work_ram, &mut pins);
    assert_eq!(board.mclk - before, cycle + 7);
    let before = board.mclk;
    board.write(work_ram, 0x55, &mut pins);
    assert_eq!(board.mclk - before, cycle); // applied once
}

/// A port device that drives the VDC by itself: at a given master clock its `catch_up` starts
/// a VRAM-to-VRAM DMA through the pins (no real device does; the socket allows it).
struct Meddler {
    at: i64,
    done: bool,
}

impl DeviceState for Meddler {}

impl PageHandler for Meddler {
    fn read(&mut self, _address: u32, _mclk: i64, _pins: &mut BoardPins) -> u8 {
        0xFF
    }

    fn write(&mut self, _address: u32, _value: u8, _mclk: i64, _pins: &mut BoardPins) {}
}

impl PortDevice for Meddler {
    fn present(&self) -> bool {
        true
    }

    fn claims(&self) -> Vec<(usize, usize)> {
        Vec::new()
    }

    fn catch_up(&mut self, mclk: i64, pins: &mut BoardPins) {
        if mclk >= self.at && !self.done {
            self.done = true;
            let vdc = &mut pins.vdcs[0];
            for (register, low, high) in
                [(0x10, 0x00, 0x10), (0x11, 0x00, 0x20), (0x12, 0x00, 0x20)]
            {
                vdc.write(0, register, mclk);
                vdc.write(2, low, mclk);
                vdc.write(3, high, mclk);
            }
        }
    }

    fn reset(&mut self, _kind: ResetKind, _pins: &mut BoardPins) {}

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[test]
fn a_port_device_that_reaches_a_vdc_from_catch_up_moves_the_horizon() {
    // Cycle by cycle, on a bare board (idle cycles: no access recomputes the horizon).
    let mut board = Board::new(BoardOptions {
        port: Some(Box::new(Meddler {
            at: 100_000,
            done: false,
        })),
        ..BoardOptions::default()
    })
    .unwrap();
    let mut pins = Pins::default();
    let mut moved = 0;
    for cycle in 0..20_000 {
        board.idle(None, &mut pins);
        let vdc = &board.vdcs[0];
        let mut eager = vdc.clone();
        eager.catch_up(board.mclk);
        assert!(
            eager == *vdc,
            "cycle {cycle}: the VDC is behind the clock {}",
            board.mclk
        );
        moved += (vdc.dma.running && vdc.regs.value[0x12] != 0x2000) as u32;
    }
    assert!(moved > 0, "the transfer the port device started never ran");
}

/// A card that counts in work RAM: `LDA #n; STA $2000` for ever (work RAM at $2000), where
/// `n` is the image's byte 5.
fn counter() -> Vec<u8> {
    let code: &[u8] = &[
        0xA9, 0xF8, 0x53, 0x02, // LDA #$F8; TAM #$02: work RAM at $2000
        0xA9, 0x11, 0x8D, 0x00, 0x20, // loop ($E004): LDA #$11; STA $2000
        0x4C, 0x04, 0xE0, // JMP loop
    ];
    let mut data = vec![0xEA; 0x2000];
    data[..code.len()].copy_from_slice(code);
    data[0x1FFE] = 0x00;
    data[0x1FFF] = 0xE0;
    data
}

fn image_of(board: &mut Board) -> &mut Vec<u8> {
    let card = board.slot.card.as_mut().expect("a card");
    &mut card
        .as_any_mut()
        .downcast_mut::<RomCard>()
        .expect("a RomCard")
        .data
}

#[test]
fn a_rom_card_has_its_image_whenever_the_host_looks() {
    let mut system = System::new(
        Board::new(BoardOptions {
            card: Some(Box::new(RomCard::new(&counter()).unwrap())),
            ..BoardOptions::default()
        })
        .unwrap(),
    );
    system.run_frame();
    assert_eq!(system.board().ram.data[0], 0x11);
    // the image is the card's, whole, between any two runs
    assert_eq!(*image_of(system.board_mut()), counter());
    system.step();
    assert_eq!(*image_of(system.board_mut()), counter());
    // and a byte the host changes in it is the byte the CPU reads
    image_of(system.board_mut())[5] = 0x22;
    system.run_frame();
    assert_eq!(system.board().ram.data[0], 0x22);
    assert_eq!(image_of(system.board_mut())[5], 0x22);
}

/// A port device that looks at the card's image at every catch-up.
struct Reader {
    looked: u64,
}

impl DeviceState for Reader {}

impl PageHandler for Reader {
    fn read(&mut self, _address: u32, _mclk: i64, _pins: &mut BoardPins) -> u8 {
        0xFF
    }

    fn write(&mut self, _address: u32, _value: u8, _mclk: i64, _pins: &mut BoardPins) {}
}

impl PortDevice for Reader {
    fn present(&self) -> bool {
        true
    }

    fn claims(&self) -> Vec<(usize, usize)> {
        Vec::new()
    }

    fn catch_up(&mut self, _mclk: i64, pins: &mut BoardPins) {
        let card = pins.card.as_mut().expect("the card");
        let image = &card.as_any_mut().downcast_mut::<RomCard>().unwrap().data;
        assert_eq!(image.len(), 0x2000, "the card's image is not with the card");
        self.looked += 1;
    }

    fn reset(&mut self, _kind: ResetKind, _pins: &mut BoardPins) {}

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[test]
fn a_rom_card_has_its_image_whenever_a_port_device_looks() {
    let mut system = System::new(
        Board::new(BoardOptions {
            card: Some(Box::new(RomCard::new(&counter()).unwrap())),
            port: Some(Box::new(Reader { looked: 0 })),
            ..BoardOptions::default()
        })
        .unwrap(),
    );
    system.run_frame();
    system.run_frame();
    let port = system.board_mut().port.as_mut().unwrap();
    let reader = port.as_any_mut().downcast_mut::<Reader>().unwrap();
    assert!(reader.looked > 10_000);
}
