// SPDX-License-Identifier: MIT
//! Issue 0004 of the reference, settled in pcengine-python 0.2.0: the board's own events run
//! while the CPU waits on a VDC access, and the PSG's level table follows `silent_units`. The
//! reference's `tests/test_vdc_background.py` (`test_a_cpu_waiting_on_a_dma_*`) and
//! `tests/test_psg.py` (`test_silent_units_set_on_an_instance_reaches_the_host_rate_output`).

use pcengine_rust::board::{Board, BoardOptions};
use pcengine_rust::cards::RomCard;
use pcengine_rust::fake_devices::{LockstepPath, ScriptedCpu};
use pcengine_rust::machine::{Machine, LINE_CLOCKS, PC_ENGINE};
use pcengine_rust::psg::Psg;
use pcengine_rust::system::System;
use pcengine_rust::vdc::regs as r;

const FIXED_263: Machine = Machine {
    lines_follow_vce_bit2: false,
    ..Machine::new("test machine, 263 lines", true)
};
const VDC_ADDR: u32 = 0x1FE000;

/// What `ST0 #reg; ST1 #lsb; ST2 #msb` does.
fn vdc_reg(cpu: &mut ScriptedCpu, register: usize, value16: u16) {
    cpu.write(VDC_ADDR, register as u8);
    cpu.write(VDC_ADDR + 2, (value16 & 0xFF) as u8);
    cpu.write(VDC_ADDR + 3, (value16 >> 8) as u8);
}

/// The 256x224 mode of pcetech 7.
fn standard_display(cpu: &mut ScriptedCpu, cr: u16) {
    vdc_reg(cpu, 0x0A, 0x0202);
    vdc_reg(cpu, 0x0B, 0x041F);
    vdc_reg(cpu, 0x0C, 0x1702);
    vdc_reg(cpu, 0x0D, 0x00DF);
    vdc_reg(cpu, 0x0E, 0x000C);
    vdc_reg(cpu, 0x05, cr);
}

fn dma(cpu: &mut ScriptedCpu, source: u16, destination: u16, length: u16, dcr: u16) {
    vdc_reg(cpu, r::DCR, dcr);
    vdc_reg(cpu, r::SOUR, source);
    vdc_reg(cpu, r::DESR, destination);
    vdc_reg(cpu, r::LENR, length); // the MSB write starts it
}

/// The (frame, line) of a time, with 263-line frames from power-on.
fn line_of(mclk: i64) -> (i64, i64) {
    let absolute = mclk / LINE_CLOCKS;
    (absolute / 263, absolute % 263)
}

/// 4,096 words started at line 20 (the display period starts at line 25), then a VRAM read,
/// which holds the CPU until the VDC serves it.
fn wait_on_a_dma_across_the_display_start(dma_resumes: bool) -> (ScriptedCpu, (i64, i64)) {
    let board = Board::new(BoardOptions {
        machine: FIXED_263,
        dma_resumes,
        ..BoardOptions::default()
    })
    .unwrap();
    let mut cpu = ScriptedCpu::new(board, true);
    standard_display(&mut cpu, 0x0080);
    for i in 0..0x1000 {
        cpu.board.vdcs[0].vram[0x1000 + i] = i as u16;
    }
    let to = LINE_CLOCKS * 263 + LINE_CLOCKS * 20 - cpu.board.mclk;
    cpu.board.advance(to);
    dma(&mut cpu, 0x1000, 0x5000, 0x0FFF, 0x0002);
    vdc_reg(&mut cpu, r::MARR, 0x0000); // accepted behind the transfer (BSY)
    assert!(cpu.board.vdcs[0].bsy());
    cpu.read(VDC_ADDR + 2); // a data read while the word is pending: the CPU waits
    let at = line_of(cpu.board.mclk);
    (cpu, at)
}

#[test]
fn a_cpu_waiting_on_a_dma_sees_the_manuals_cancel_at_the_display_start() {
    let (mut cpu, at) = wait_on_a_dma_across_the_display_start(false);
    assert_eq!(at, (1, 25)); // released as the display period begins, not ~25 lines on
    let vdc = &cpu.board.vdcs[0];
    assert!(vdc.regs.value[r::LENR] != 0xFFFF && vdc.status & r::DV == 0);
    assert!(!vdc.dma.running && !vdc.bsy());
    let left = vdc.regs.value[r::LENR];
    cpu.board.advance(LINE_CLOCKS * 263);
    assert_eq!(cpu.board.vdcs[0].regs.value[r::LENR], left); // cancelled, not resumed
}

#[test]
fn a_cpu_waiting_on_a_dma_is_held_through_the_display_while_it_is_paused_c7() {
    let (cpu, at) = wait_on_a_dma_across_the_display_start(true);
    let vdc = &cpu.board.vdcs[0];
    assert!(vdc.regs.value[r::LENR] == 0xFFFF && vdc.status & r::DV != 0);
    assert_eq!(at, (2, 5)); // 5 lines' worth before the display, the other 19 after it
    assert!(cpu.board.capture.last_frame.is_some() && cpu.board.timeline.frame == 2);
    let copied: Vec<u16> = [0, 0x800, 0xFFF]
        .iter()
        .map(|&i| vdc.vram[0x5000 + i])
        .collect();
    assert_eq!(copied, [0, 0x800, 0xFFF]);
}

/// The clean rewrite (2026-11-19; the reference's chip.py `write`): DVSSR's high byte arms the
/// sprite-table transfer [mfr: "triggered by access to the high byte"] and its low byte alone
/// does not. Checked here on its own because the Rust code this replaced was withdrawn unread,
/// so whether it ever armed on the low byte is not known from it; the reference states the rule.
#[test]
fn only_dvssrs_high_byte_arms_the_sprite_table_transfer() {
    for (high, copied) in [(false, 0u16), (true, 0x1234)] {
        let board = Board::new(BoardOptions {
            machine: FIXED_263,
            ..BoardOptions::default()
        })
        .unwrap();
        let mut cpu = ScriptedCpu::new(board, true);
        standard_display(&mut cpu, 0x0080);
        cpu.board.vdcs[0].vram[0x7F00] = 0x1234;
        cpu.write(VDC_ADDR, r::SATB as u8);
        if high {
            cpu.write(VDC_ADDR + 2, 0x00);
            cpu.write(VDC_ADDR + 3, 0x7F);
        } else {
            cpu.board.vdcs[0].regs.value[r::SATB] = 0x7F00; // the address, without a write to it
            cpu.write(VDC_ADDR + 2, 0x00);
        }
        cpu.board.advance(LINE_CLOCKS * 263);
        assert_eq!(
            cpu.board.vdcs[0].sat[0], copied,
            "high byte written: {high}"
        );
    }
}

fn w(psg: &mut Psg, reg: u32, value: u8) {
    let at = 6 * psg.step;
    psg.write(reg, value, at);
}

#[test]
fn silent_units_set_on_an_instance_reaches_the_host_rate_output() {
    let mut outputs = Vec::new();
    for units in [30, 31] {
        let mut psg = Psg::default();
        psg.switches.silent_units = units; // on the instance, after construction
        w(&mut psg, 0, 0);
        w(&mut psg, 4, 0x00);
        for _ in 0..32 {
            w(&mut psg, 6, 31);
        }
        w(&mut psg, 1, 0xFF); // main volume at maximum
        w(&mut psg, 0, 0);
        w(&mut psg, 2, 10);
        w(&mut psg, 3, 0);
        w(&mut psg, 5, 0xFF);
        w(&mut psg, 4, 0x80 | 0x01); // AL 1: 30 units, main and pan at maximum
        assert_eq!(psg.attenuation(0, 0), 30);
        let at = 6 * psg.step;
        psg.take_samples(at); // drop the setup
        let at = 6 * (psg.step + 2000);
        outputs.push(psg.take_samples(at));
    }
    assert!(outputs[0].iter().all(|&sample| sample == (0, 0))); // 45 dB: silent (the manual)
    assert!(outputs[1].iter().any(|&sample| sample != (0, 0))); // 46.5 dB still audible
}

/// The lockstep pixel path hands `ST0`-`ST2` to both VDCs in one access. When the second one
/// has to wait and the first did not, the access stops at the second (sockets.rs,
/// `VdcAccess`): when the board makes the decode again after the wait, the first VDC's write is
/// handed back from the log, not made twice. VDC 1 gets CPU slots in the display (MWR mode 0),
/// VDC 2 none (mode 3), so every `ST2` after the first in a display period waits on VDC 2.
#[test]
fn a_decode_made_again_after_a_wait_does_not_repeat_the_accesses_made_before_it() {
    #[rustfmt::skip]
    let code: &[u8] = &[
        0x78, 0xD4, // SEI; CSH
        0xA9, 0xFF, 0x53, 0x01, // LDA #$FF; TAM #$01: the hardware page at $0000
        0x03, 0x0A, 0x13, 0x02, 0x23, 0x02, // both: HSR
        0x03, 0x0B, 0x13, 0x1F, 0x23, 0x04, // HDR
        0x03, 0x0C, 0x13, 0x02, 0x23, 0x17, // VPR
        0x03, 0x0D, 0x13, 0xDF, 0x23, 0x00, // VDW
        0x03, 0x0E, 0x13, 0x0C, 0x23, 0x00, // VCR
        0x03, 0x05, 0x13, 0x80, 0x23, 0x00, // CR: the background on
        0x03, 0x09, 0x13, 0x03, 0x23, 0x00, // MWR: mode 3, no CPU slot in the display
        0xA9, 0x09, 0x8D, 0x00, 0x00, // VDC 1 alone (a plain store): select MWR
        0xA9, 0x00, 0x8D, 0x02, 0x00, 0x8D, 0x03, 0x00, // MWR 0: CPU slots
        0x03, 0x00, 0x13, 0x00, 0x23, 0x00, // both: MAWR 0
        0x03, 0x02, // both: select VWR
        0x13, 0x11, 0x23, 0x22, // loop: ST1 #$11; ST2 #$22
        0x80, 0xFA, // BRA loop
    ];
    let mut image = vec![0xEA; 0x2000];
    image[..code.len()].copy_from_slice(code);
    image[0x1FFE] = 0x00;
    image[0x1FFF] = 0xE0;
    let board = Board::new(BoardOptions {
        machine: Machine {
            vdc_count: 2,
            ..PC_ENGINE
        },
        card: Some(Box::new(RomCard::new(&image).unwrap())),
        pixel_path: Some(Box::new(LockstepPath { lines_seen: 0 })),
        ..BoardOptions::default()
    })
    .unwrap();
    let mut system = System::new(board);
    for _ in 0..3 {
        system.run_frame();
    }
    let vdcs = &system.board().vdcs;
    let (first, second) = (vdcs[0].regs.value[r::MAWR], vdcs[1].regs.value[r::MAWR]);
    assert!(second > 1000, "the loop ran: {second} writes");
    assert_eq!(first, second, "each ST2 reaches each VDC once");
}

/// A VRAM write the CPU waited for is made where the wait ended (the reference's
/// `_data_access`: `mclk = max(served, mclk)`). With MWR's access mode 3 the CPU has no slot in
/// the display, so a first VWR write is pending until the display width ends; a second waits
/// for it, and is made after the width, where it commits at once: not pending past the access.
#[test]
fn a_waited_vram_write_is_made_where_the_wait_ended() {
    let board = Board::new(BoardOptions {
        machine: FIXED_263,
        ..BoardOptions::default()
    })
    .unwrap();
    let mut cpu = ScriptedCpu::new(board, true);
    standard_display(&mut cpu, 0x0080);
    // access mode 3: no CPU slot during the display
    vdc_reg(&mut cpu, r::MWR, 0x0003);
    // into frame 1, line 40 (a display line), 600 master clocks after its start: in the width
    let to = LINE_CLOCKS * (263 + 40) + 600 - cpu.board.mclk;
    cpu.board.advance(to);
    vdc_reg(&mut cpu, r::MAWR, 0x0100);
    cpu.write(VDC_ADDR, r::VRW as u8);
    cpu.write(VDC_ADDR + 2, 0x34);
    cpu.write(VDC_ADDR + 3, 0x12); // accepted, pending to the width's end (BSY)
    assert!(cpu.board.vdcs[0].bsy());
    let before = cpu.board.mclk;
    cpu.write(VDC_ADDR + 2, 0x78);
    cpu.write(VDC_ADDR + 3, 0x56); // waits for the first, then is made after the width
    assert!(cpu.board.mclk > before + 100, "the CPU was held");
    let vdc = &cpu.board.vdcs[0];
    assert!(
        !vdc.bsy(),
        "made where the wait ended, outside the width: committed at once"
    );
    assert_eq!((vdc.vram[0x100], vdc.vram[0x101]), (0x1234, 0x5678));
}

/// MWR's sprite access width (SM) is taken at the vertical blanking, not live [mfr: MWR "valid
/// at the beginning of a vertical blanking period"; the reference's `fetch_sprites` reads
/// `_mwr_cache | (mwr & 0x80)`]. A sprite whose pattern has only plane 2 set shows colour 4 with
/// four planes (SM 0) and nothing with SM 1's two; SM 1 written during the display period does
/// not reach this frame's sprites.
#[test]
fn the_sprite_access_width_waits_for_the_vertical_blanking() {
    let board = Board::new(BoardOptions {
        machine: FIXED_263,
        ..BoardOptions::default()
    })
    .unwrap();
    let mut cpu = ScriptedCpu::new(board, true);
    standard_display(&mut cpu, 0x0040); // sprites only
    {
        let vdc = &mut cpu.board.vdcs[0];
        vdc.sat[..4].copy_from_slice(&[64 + 50, 32, 0x100 << 1, 0]); // display line 50, column 0
        for row in 0..16 {
            vdc.vram[0x100 * 64 + 32 + row] = 0xFFFF; // plane 2 only
        }
    }
    // the VCE's colour 0x104 (sprite palette 0, colour 4) set to a value the row shows
    for (offset, value) in [(0x402, 0x04), (0x403, 0x01), (0x404, 0x55), (0x405, 0x01)] {
        cpu.write(VDC_ADDR + offset, value);
    }
    cpu.board.advance(LINE_CLOCKS * (263 + 40) - cpu.board.mclk); // frame 1, display line 15
    vdc_reg(&mut cpu, r::MWR, 0x0004); // SM 1, in the display period
    cpu.board
        .advance(LINE_CLOCKS * (2 * 263 + 5) - cpu.board.mclk);
    let frame = cpu.board.capture.last_frame.as_ref().unwrap();
    let row = &frame.rows[(25 + 50 - 14) as usize];
    assert_eq!(
        row[24], 0x155,
        "the frame's sprites keep the width the last vblank took"
    );
}
