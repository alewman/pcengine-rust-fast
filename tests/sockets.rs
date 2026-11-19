// SPDX-License-Identifier: MIT
//! The sockets (the reference's docs/sockets.md), exercised with the test-only fakes: every
//! trait has a fake and the base board works with each of them plugged in, and unchanged
//! without. The reference's tests/test_sockets.py, test for test; what it checks of Python's
//! structural typing is the compiler's here.

use pcengine_rust::board::{Board, BoardOptions};
use pcengine_rust::fake_devices::{
    FakeAudioSource, FakeCard, FakeController, FakePixelPath, FakePort, ScriptedCpu,
};
use pcengine_rust::machine::{Machine, LINE_CLOCKS, PC_ENGINE};
use pcengine_rust::sockets::{
    AudioSource, Card, CardSlot, ControllerPort, DeviceState, DirectPath, IrqLine, PixelPath,
    PortDevice, VideoTiming,
};
use pcengine_rust::vce::Vce;
use pcengine_rust::vdc::Vdc;

fn board(options: BoardOptions) -> Board {
    Board::new(options).unwrap()
}

#[test]
fn fakes_satisfy_the_traits() {
    let _card: Box<dyn Card> = Box::new(FakeCard::new(vec![]));
    let _port: Box<dyn PortDevice> = Box::new(FakePort::new(vec![]));
    let _source: Box<dyn AudioSource> = Box::new(FakeAudioSource);
    let _pad: Box<dyn ControllerPort> = Box::new(FakeController::new(0x05));
    let _path: Box<dyn PixelPath> = Box::new(FakePixelPath::default());
}

#[test]
fn irq_line_is_a_wired_or_of_sources_never_a_max() {
    let mut line = IrqLine::new("IRQ2");
    assert!(!line.level());
    line.set("card", true);
    line.set("cd-adpcm", true);
    assert!(line.level());
    assert_eq!(
        line.sources().iter().collect::<Vec<_>>(),
        ["card", "cd-adpcm"]
    );
    line.set("card", false);
    assert!(line.level()); // the CD still holds it
    line.set("cd-adpcm", false);
    assert!(!line.level());
}

#[test]
fn empty_slot_is_a_state_and_the_card_decides_grounds_cd() {
    assert!(!CardSlot::default().grounds_cd());
    assert!(CardSlot::new(Some(Box::new(FakeCard::new(vec![])))).grounds_cd());
    let mut loose = FakeCard::new(vec![]);
    loose.grounds_cd = false;
    assert!(!CardSlot::new(Some(Box::new(loose))).grounds_cd());
}

#[test]
fn direct_path_is_vdc_one_straight_into_the_vce() {
    let timing = VideoTiming {
        frame: 0,
        line: 3,
        start_mclk: 0,
        dot_divider: 4,
        frame_lines: 263,
        in_vsync: false,
    };
    let rows = vec![vec![1, 2, 3], vec![9, 9, 9]];
    assert_eq!(DirectPath.mix(&timing, rows), [1, 2, 3]);
}

fn fake_path(board: &mut Board) -> &mut FakePixelPath {
    board
        .pixel_path
        .as_any_mut()
        .downcast_mut::<FakePixelPath>()
        .unwrap()
}

#[test]
fn a_replacement_pixel_path_sees_every_line() {
    let mut board = board(BoardOptions {
        pixel_path: Some(Box::new(FakePixelPath::default())),
        ..BoardOptions::default()
    });
    board.advance(LINE_CLOCKS * 10 + 237); // ten rows have ended
    assert_eq!(fake_path(&mut board).lines_seen, 10);
}

#[test]
fn a_replacement_pixel_path_changes_the_picture() {
    struct Marker;
    impl DeviceState for Marker {}
    impl PixelPath for Marker {
        fn mix(&mut self, _timing: &VideoTiming, rows: Vec<Vec<u16>>) -> Vec<u16> {
            vec![0x1FF; rows[0].len()]
        }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }
    let mut board = board(BoardOptions {
        pixel_path: Some(Box::new(Marker)),
        ..BoardOptions::default()
    });
    board.vce.write(2, 0xFF, 0);
    board.vce.write(3, 0x01, 0);
    board.vce.write(4, 0x23, 0);
    board.vce.write(5, 0x01, 0);
    board.advance(LINE_CLOCKS * 263 * 2);
    let frame = board.capture.last_frame.as_ref().unwrap();
    assert_eq!(frame.rows.len(), 242);
    assert!(frame.rows.iter().flatten().all(|&value| value == 0x123));
}

#[test]
fn video_timing_record_per_line_has_the_dot_divider() {
    let mut board = board(BoardOptions {
        pixel_path: Some(Box::new(FakePixelPath::default())),
        ..BoardOptions::default()
    });
    board.vce.write(0, 0x01, 0); // 7 MHz
    board.advance(LINE_CLOCKS * 3 + 237);
    let seen = fake_path(&mut board).timings.clone();
    assert_eq!(seen.iter().map(|t| t.line).collect::<Vec<_>>(), [0, 1, 2]);
    assert!(seen[1..].iter().all(|t| t.dot_divider == 3));
    assert_eq!(seen[2].start_mclk, 2 * LINE_CLOCKS);
}

#[test]
fn audio_source_list_has_the_psg_first() {
    let mut board = board(BoardOptions::default());
    board.audio_sources.push(Box::new(FakeAudioSource));
    assert_eq!(board.audio_sources[0].take_samples(0), [(1, -1)]);
    // the mixer takes the PSG first, then the list: one sample from the fake bounds the count
    board.advance(21_477_272 / 10);
    let mixed = board.take_audio();
    // PSG silent (0, 0) + (1, -1) * 0.5: Python's round() takes 0.5 and -0.5 to the even 0
    assert_eq!(mixed, [(0, 0)]);
}

fn fake_port(board: &mut Board) -> &mut FakePort {
    board
        .port
        .as_mut()
        .unwrap()
        .as_any_mut()
        .downcast_mut::<FakePort>()
        .unwrap()
}

#[test]
fn a_port_device_is_caught_up_by_the_board_when_it_asks() {
    let board = board(BoardOptions {
        port: Some(Box::new(FakePort::new(vec![(0x1FF8, 0x1FF8)]))),
        ..BoardOptions::default()
    });
    let mut cpu = ScriptedCpu::new(board, false);
    assert_eq!(cpu.read(0x1FF800), 0xCC);
    // the access carries the master-clock time; the device may run itself up to it
    let mclk = cpu.board.mclk;
    assert_eq!(fake_port(&mut cpu.board).reads[0].1, mclk);
    // K7: its presence pulls the I/O port's D7 low
    assert_eq!(cpu.read(0x1FF000) & 0x80, 0);
}

/// Issue 0002 of the reference: the board used to call `catch_up` on the VDCs and the timer
/// only, so a port device (a CD drive's REQ, an ADPCM end) saw time at its own register
/// accesses and could not raise IRQ2 between them.
#[test]
fn a_port_device_runs_with_the_board_clock_and_can_raise_irq2_by_itself() {
    let mut port = FakePort::new(vec![(0x1FF8, 0x1FF8)]);
    port.alarm_due = Some(5000);
    let mut board = board(BoardOptions {
        port: Some(Box::new(port)),
        ..BoardOptions::default()
    });
    assert!(!board.irq2.level());
    board.advance(4000);
    assert_eq!(fake_port(&mut board).caught_up.last(), Some(&4000));
    assert!(!board.irq2.level());
    board.advance(1000); // no access to the device at all: the clock alone reaches it
    assert_eq!(fake_port(&mut board).caught_up.last(), Some(&5000));
    assert!(board.irq2.level());
    assert!(board.lines().1);
}

#[test]
fn irq2_from_a_port_device_reaches_the_cpu_pins() {
    let mut port = FakePort::new(vec![(0x1FF8, 0x1FF8)]);
    port.alarm_due = Some(100);
    let board = board(BoardOptions {
        port: Some(Box::new(port)),
        ..BoardOptions::default()
    });
    let mut cpu = ScriptedCpu::new(board, false);
    assert_eq!(cpu.pins.irq_lines(), (false, false, false));
    cpu.idle(10);
    assert_eq!(cpu.pins.irq_lines(), (false, true, false));
    // $1403 shows the line whatever the mask
    assert_eq!(cpu.read(0x1FF403) & 7, 0b001);
}

#[test]
fn instances_not_modules_two_boards_two_vdcs_do_not_share_state() {
    let (mut a, b) = (
        board(BoardOptions::default()),
        board(BoardOptions::default()),
    );
    a.vdcs[0].vram[5] = 0x1234;
    a.vce.palette[5] = 7;
    a.ram.data[3] = 9;
    a.psg.write(1, 0xFF, 0);
    assert_eq!(
        (b.vdcs[0].vram[5], b.vce.palette[5], b.ram.data[3]),
        (0, 0, 0)
    );
    assert_eq!(b.psg.balance, 0);
    let mut v1 = Vdc::new(None, Some(-16), true);
    let v2 = Vdc::new(None, Some(-16), true);
    v1.regs.value[5] = 1;
    assert_eq!(v2.regs.value[5], 0);
    assert_eq!(Vce::new(None).palette.len(), 512);
}

#[test]
fn psg_writes_through_the_bus_reach_the_audio_source_list_with_their_time() {
    let mut cpu = ScriptedCpu::new(board(BoardOptions::default()), true);
    let base = 0x1FE800;
    cpu.write(base + 1, 0xFF); // R1: main level
    cpu.write(base, 0x00); // R0: channel 1
    cpu.write(base + 4, 0x00); // ON = 0, DDA = 0: write data mode
    for i in 0..32 {
        cpu.write(base + 6, if i < 16 { 31 } else { 0 });
    }
    cpu.write(base + 2, 0x20);
    cpu.write(base + 3, 0x00);
    cpu.write(base + 5, 0xFF);
    cpu.write(base + 4, 0x9F); // ON, full level
    cpu.board.advance(3 * 21_477_272 / 100); // about 30 ms
    let mclk = cpu.board.mclk;
    let samples = cpu.board.psg.take_samples(mclk);
    assert!((1200..=1400).contains(&samples.len())); // 44.1 kHz
    let left: Vec<i32> = samples.iter().map(|s| s.0).collect();
    // a square wave about the centre
    assert!(*left.iter().max().unwrap() > 0 && *left.iter().min().unwrap() < 0);
    assert_eq!(cpu.board.cpu_io.buffer.value, 0x9F); // the I/O buffer saw the last write
}

// --- a card that claims pages, RAM included ------------------------------------------------------

#[test]
fn a_card_claims_any_pages_and_a_claim_replaces_the_base_handler() {
    // the ROM window and the work RAM's first bank
    let card = FakeCard::new(vec![(0x0000, 0x0FFF), (0x1F00, 0x1F1F)]);
    let mut cpu = ScriptedCpu::new(
        board(BoardOptions {
            card: Some(Box::new(card)),
            ..BoardOptions::default()
        }),
        false,
    );
    assert_eq!(cpu.read(0x000123), 0xAA);
    assert_eq!(cpu.read(0x1F0005), 0xAA); // the card answers where the RAM did
    cpu.write(0x1F0005, 0x42);
    cpu.write(0x1F2005, 0x42); // bank $F9: still the board's RAM (a mirror of $F8)
    assert_eq!(cpu.read(0x1F2005), 0x42);
    assert_eq!(cpu.board.ram.data[5], 0x42);
    let card = cpu.board.slot.card.as_mut().unwrap().as_any_mut();
    let card = card.downcast_mut::<FakeCard>().unwrap();
    assert_eq!(card.writes.len(), 1);
    assert_eq!((card.writes[0].0, card.writes[0].1), (0x1F0005, 0x42));
    assert_eq!(card.reads.len(), 2);
}

#[test]
fn an_empty_slot_answers_ff_and_a_pad_answers_on_the_port() {
    let pad = FakeController::new(0x05);
    let mut cpu = ScriptedCpu::new(
        board(BoardOptions {
            pad: Some(Box::new(pad)),
            ..BoardOptions::default()
        }),
        false,
    );
    assert_eq!(cpu.read(0x000000), 0xFF);
    assert_eq!(cpu.read(0x1FF000), 0x80 | 0x40 | 0x30 | 0x05);
    cpu.write(0x1FF000, 0x02);
    let pad = cpu.board.cpu_io.port.device.as_mut().unwrap().as_any_mut();
    assert_eq!(
        pad.downcast_mut::<FakeController>().unwrap().writes,
        [(false, true)]
    );
}

// --- 12. Instances: a machine row with two VDCs (the SuperGrafx add-on, the reference's issue 0001)

#[test]
fn one_vdc_is_the_default_and_the_board_holds_a_list() {
    assert_eq!(board(BoardOptions::default()).vdcs.len(), 1);
}

#[test]
fn a_second_vdc_is_run_synced_read_for_pixels_and_ored_into_irq1() {
    let machine = Machine {
        vdc_count: 2,
        ..PC_ENGINE
    };
    let board = board(BoardOptions {
        machine,
        pixel_path: Some(Box::new(FakePixelPath::default())),
        ..BoardOptions::default()
    });
    assert_eq!(board.vdcs.len(), 2);
    let mut cpu = ScriptedCpu::new(board, false);
    // the board itself decodes only VDC 1; run a few lines and check VDC 2 was synced with it
    cpu.idle((3 * LINE_CLOCKS / 12) as usize);
    let counts = fake_path(&mut cpu.board).row_counts.clone();
    assert!(!counts.is_empty() && counts.iter().all(|&n| n == 2));
    assert_eq!(cpu.board.vdcs[0].seq.origin, cpu.board.vdcs[1].seq.origin);
    // IRQ1 is the wired-OR of both: raise VDC 2's status bit and the line follows
    assert!(!cpu.board.lines().0);
    cpu.board.vdcs[1].status |= 0x20; // VD
    assert!(cpu.board.lines().0);
    cpu.idle(1);
    assert_eq!(cpu.pins.irq_lines(), (true, false, false));
    cpu.board.vdcs[1].status = 0;
    cpu.board.vdcs[0].status |= 0x04; // RR
    assert!(cpu.board.lines().0);
}
