// SPDX-License-Identifier: MIT
//! Pads against pcetech section 4 (2-button, Turbo Tap) and pce-devel's PCE_Controller_Info
//! (6-button scans, multitap port table): the reference's tests/test_pads.py.

use pcengine_rust::pads::{self, MultiTap, Pad2, Pad6};
use pcengine_rust::sockets::ControllerPort;

const DIRS: u16 = pads::UP | pads::RIGHT | pads::DOWN | pads::LEFT;

#[test]
fn two_button_sel_high_reads_directions_active_low() {
    let mut pad = Pad2::new();
    assert_eq!(pad.read(true, false), 0x0F); // nothing pressed: all open = 1
    pad.pressed = pads::UP;
    assert_eq!(pad.read(true, false), 0x0E);
    pad.pressed = pads::LEFT | pads::RIGHT;
    assert_eq!(pad.read(true, false), 0b0101);
    pad.pressed = pads::DOWN;
    assert_eq!(pad.read(true, false), 0b1011);
}

#[test]
fn two_button_sel_low_reads_buttons() {
    let mut pad = Pad2::new();
    pad.pressed = pads::I;
    assert_eq!(pad.read(false, false), 0b1110); // D0 = button I
    pad.pressed = pads::II | pads::SELECT;
    assert_eq!(pad.read(false, false), 0b1001); // D1 = II, D2 = Select
    pad.pressed = pads::RUN;
    assert_eq!(pad.read(false, false), 0b0111); // D3 = Run
    pad.pressed = pads::UP | pads::RUN; // the other group does not leak in
    assert_eq!(pad.read(false, false), 0b0111);
}

#[test]
fn two_button_clr_high_returns_zeros() {
    let mut pad = Pad2::new();
    pad.pressed = DIRS;
    assert_eq!(pad.read(true, true), 0);
    assert_eq!(pad.read(false, true), 0);
}

#[test]
fn six_button_alternates_scans_on_clr_pulses() {
    let mut pad = Pad6::new();
    pad.pressed = pads::UP | pads::I | pads::III | pads::VI;
    let first = (pad.read(true, false), pad.read(false, false));
    assert_eq!(first, (0b1110, 0b1110)); // directions; I
    pad.write(true, true); // CLR 0 -> 1
    pad.write(true, false);
    let second = (pad.read(true, false), pad.read(false, false));
    assert_eq!(second, (0b0000, 0b0110)); // SEL high shows 0000; SEL low III (D0) and VI (D3)
    pad.write(true, true);
    pad.write(true, false);
    assert_eq!((pad.read(true, false), pad.read(false, false)), first);
}

#[test]
fn six_button_ignores_repeated_clr_high() {
    let mut pad = Pad6::new();
    pad.write(true, true);
    pad.write(true, true);
    pad.write(true, false);
    assert!(pad.second_scan);
}

fn make_tap(connected: &[usize], clr_high_reads_pad: bool) -> MultiTap {
    let ports = (0..5)
        .map(|i| {
            connected.contains(&i).then(|| {
                // port i holds a distinct key down
                Box::new(Pad2 { pressed: 1 << i }) as Box<dyn ControllerPort>
            })
        })
        .collect();
    let mut tap = MultiTap::new(ports).unwrap();
    tap.clr_high_reads_pad = clr_high_reads_pad;
    tap
}

/// The game's sequence from PCE_Controller_Info: $01, $03, $01.
fn reset_sequence(tap: &mut MultiTap) {
    tap.write(true, false);
    tap.write(true, true);
    tap.write(true, false);
}

#[test]
fn multitap_reads_five_pads_in_order_then_zeros() {
    let mut tap = make_tap(&[0, 1, 2, 3, 4], true);
    reset_sequence(&mut tap);
    for i in 0..5 {
        // port i holds key bit i down: UP, RIGHT, DOWN, LEFT on SEL high, then button I
        let direction = tap.read(true, false);
        assert_eq!(direction, if i < 4 { !(1u8 << i) & 0x0F } else { 0x0F });
        tap.write(false, false);
        assert_eq!(tap.read(false, false), if i < 4 { 0x0F } else { 0b1110 });
        tap.write(true, false); // SEL 0 -> 1 with CLR low: next port
    }
    assert_eq!(tap.counter, 5);
    assert_eq!(tap.read(true, false), 0); // "returns $00 until the counter is reset"
    assert_eq!(tap.read(false, false), 0);
}

#[test]
fn multitap_unconnected_port_returns_0f() {
    let mut tap = make_tap(&[0], true);
    reset_sequence(&mut tap);
    tap.write(false, false);
    tap.write(true, false);
    assert_eq!(tap.read(true, false), 0x0F);
    assert_eq!(tap.read(false, false), 0x0F);
}

/// [hw: pcetech 4] 'When $01 is written to $1000, D3-D0 are returned as zero'.
#[test]
fn multitap_first_01_write_returns_zeros_when_counter_is_spent() {
    let mut tap = make_tap(&[0, 1, 2, 3, 4], true);
    tap.write(false, false);
    tap.write(true, false);
    assert_eq!(tap.read(true, false), 0);
}

#[test]
fn multitap_clr_high_after_reset_shows_pad_one_per_pcetech() {
    let mut tap = make_tap(&[0, 1, 2, 3, 4], true);
    tap.write(true, false);
    tap.write(true, true); // $03
    assert_eq!(tap.read(true, true), Pad2 { pressed: 1 }.read(true, false)); // C10: pcetech's reading
}

#[test]
fn multitap_c12_alternative_reads_zero_with_clr_high() {
    let mut tap = make_tap(&[0, 1, 2, 3, 4], false);
    tap.write(true, false);
    tap.write(true, true);
    assert_eq!(tap.read(true, true), 0);
}

#[test]
fn multitap_reset_needs_sel_high() {
    let mut tap = make_tap(&[0, 1, 2, 3, 4], true);
    reset_sequence(&mut tap);
    tap.write(false, false);
    tap.write(true, false); // port 2
    assert_eq!(tap.counter, 1);
    tap.write(false, false); // SEL low: a CLR rise now must NOT reset
    tap.write(false, true);
    assert_eq!(tap.counter, 1);
    tap.write(true, true); // SEL rises with CLR already high: no increment
    assert_eq!(tap.counter, 1);
}

#[test]
fn a_multitap_has_five_ports() {
    assert!(MultiTap::new(Vec::new()).is_err());
}
