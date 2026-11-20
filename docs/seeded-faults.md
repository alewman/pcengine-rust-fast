<!-- SPDX-License-Identifier: MIT -->
# Seeded faults (rung 7)

**Rerun 2026-11-19 after merging pcengine-rust `37f17ba`** (the early display start crop, against
pcengine-python `03ac699`'s cache, rebuilt here): **80 caught, 3 missed**, the same count as the
run below. Log: `external/gate-2026-11-19-03ac699.log` (not committed; the ladder keeps only the
last lines, so the caught and missed lists were not compared name by name).

**This fork, 2026-11-19 (the clean rewrite merged, against pcengine-python `3318027`'s rebuilt
reference cache): 80 caught, 3 missed** of 83: pcengine-rust's 66 (its table below), each caught
by the same check as on pcengine-rust, its three misses included (the board's wait, below), and
this fork's seventeen ("This fork's shortcuts", at the end). Log: `external/faults-2026-11-19.log`
(not committed). The run was made on the tree before the inert row-buffer reuse was taken out
(docs/not-kept.md); its fault `row-buffer-not-cleared` was the fourth miss, an equivalent fault,
and is gone with the three lines; the other 83 are unaffected by that change, which leaves every
bus cycle, pixel and allocation as it was (the gate was re-run on it). The seeder now copies the
tree with fresh file times (with copytree's `copy2`, a file restored after a fault kept its old
time and cargo could keep the faulted build of it); a first run without that fix was stopped and
discarded.

pcengine-rust's record follows.

A gate that cannot fail proves nothing. This is the record of the run on 2026-11-19 (the clean
rewrite, against pcengine-python `3318027`'s rebuilt reference cache): the 49 faults of 0.2.0 that
did not hold withdrawn code (seven did, and were cut unread with it: docs/restart.md) and seventeen
new ones on the new code: the line machine (`display-start-a-character-late`,
`scroll-latched-a-character-early`, `vds-a-line-short`, `front-porch-vcr-plus-two`,
`raster-counter-one-at-the-display`, `burst-from-the-background-bit`,
`cut-display-loses-its-vblank`), the scroll counter (`byr-reloads-on-the-msb-only`,
`scroll-counter-not-stepped`), the DMAs (`sat-transfer-two-lines`, `dma-cancelled-by-the-display`,
`vram-dma-one-dot-a-word`), the sprite budget and MWR (`sprite-fetch-budget-ignored`,
`sprite-search-budget-ignored`, `mwr-fields-live`), the write port's stop for the board's wait
(`vwr-write-not-stopped-for-the-wait`) and the re-cited sync width (`hsync-a-clock-short`).

`scripts/seeded_faults.py` plants one single-line fault at a time in a copy
of `src/`, builds it, and runs the checks cheapest first until one fails:
rung 2 (Covell's test and the probe card), rung 5 (the socket, machine and
switch cases), the smoke list of rung 4, `cargo test`, rung 3, the whole
library, rung 6. Since 2026-11-19 it first checks that the unfaulted board
passes rung 2 (a case that cannot run, such as a missing image, fails like a
difference and "catches" every fault), and it copies the README into its tree
(`src/lib.rs` runs the README's example as a doctest; without it `cargo test`
failed whatever the fault, so every fault that reached the tests counted as
caught: the five "a test the script could not name" of the 0.2.0 record
below were that).

| Fault | Module | What it changes | Caught by |
| --- | --- | --- | --- |
| `vce-access-not-stretched` | `board.rs` | only a VDC access takes the extra cycle, not a VCE one (C4, ares' reading) | rung 2 (`covell`) |
| `row-ends-a-dot-late` | `board.rs` | a line's picture row is handed to the capture 4 master clocks late | rung 2 (`covell`) |
| `irq1-from-the-first-vdc-only` | `board.rs` | a second VDC's interrupt is not ORed into IRQ1 (the reference's issue 0001) | `cargo test` (`a_second_vdc_is_run_synced_read_for_pixels_and_ored_into_irq1`) |
| `port-device-not-caught-up` | `board.rs` | the board never runs the port device up to its clock (the reference's issue 0002) | rung 5 (`sock-port-lazers`) |
| `disable-mask-not-delivered` | `board.rs` | a write to $1402 never reaches the CPU | rung 5 (`sock-multitap-bomberman94`) |
| `lines-delivered-only-on-advance` | `board.rs` | an interrupt line an access changed is seen by the CPU a cycle later | rung 2 (`base-probe`) |
| `vdc-waits-alone` | `board.rs` | a waiting VDC runs alone to the transfer's end, lines ahead of the line clock (v0.1.0) | rung 2 (`base-probe`) |
| `served-access-made-at-the-old-time` | `board.rs` | the access a VDC waited for is made at the time the wait began, not where it ended | **MISSED** |
| `wait-not-held` | `board.rs` | the CPU is not held while the board serves a VDC's access | rung 2 (`base-probe`) |
| `wait-not-settled` | `board.rs` | after the wait the devices are not caught up to the time it ended | **MISSED** |
| `decode-again-repeats-accesses` | `sockets.rs` | a decode made again after a wait repeats the VDC accesses made before the stop | `cargo test` (`a_decode_made_again_after_a_wait_does_not_repeat_the_accesses_made_before_it`) |
| `accesses-after-a-stop-made` | `sockets.rs` | the VDC accesses after a stop are made before the wait, not after it | **MISSED** |
| `level-table-built-once` | `psg/chip.rs` | `silent_units` set after construction leaves the host-rate level table (v0.1.0) | `cargo test` (`silent_units_set_on_an_instance_reaches_the_host_rate_output`) |
| `timer-prescaler-not-reset` | `cpu_io/timer.rs` | the first tick after an enable comes a CPU cycle early (U24) | rung 2 (`base-probe`) |
| `timer-window-a-clock-long` | `cpu_io/timer.rs` | the count reads $7F one master clock longer after an underflow (C25) | rung 2 (`base-probe`) |
| `irq-status-buffer-bits` | `cpu_io/irq_ctl.rs` | $1402/$1403 return only four bits of the I/O buffer | rung 2 (`base-probe`) |
| `io-port-d4-low` | `cpu_io/io_port.rs` | the I/O port's D4 reads 0 | rung 2 (`covell`) |
| `read-does-not-load-the-buffer` | `cpu_io/mod.rs` | a read of the timer, the port or the interrupt registers leaves the I/O buffer | rung 2 (`base-probe`) |
| `pad6-counts-clr-level` | `pads.rs` | the 6-button pad changes scan on every write with CLR high, not on its rise | `cargo test` (`six_button_ignores_repeated_clr_high`) |
| `multitap-counts-under-clr` | `pads.rs` | the multitap's counter steps on a SEL rise while CLR is high | rung 5 (`sock-multitap-probe`) |
| `card-384k-upper-mirror` | `cards.rs` | a 384 KiB card's banks $40-$7F show the wrong part of the image | rung 5 (`sock-card-bonk`) |
| `tennokoe-lock-inverted` | `cards.rs` | the Tennokoe card's RAM unlocks on a zero | rung 5 (`sock-tennokoe-probe`) |
| `populous-ram-two-banks` | `cards.rs` | the Populous card's RAM takes writes in two banks of its four | rung 4 (smoke) (`lib-Populous (Japan) (En)`) |
| `palette-lsb-clears-bit-8` | `vce.rs` | a write of a colour's low byte clears its ninth bit | rung 2 (`covell`) |
| `palette-write-a-dot-late` | `video.rs` | a palette write made exactly at a dot's start shows from the next dot | rung 2 (`covell`) |
| `line-count-ignores-the-machine-row` | `video.rs` | `lines_follow_vce_bit2 = false` still follows VCE bit 2 (C3) | rung 5 (`sw-no-bit2-lines-probe`) |
| `cpu-slot-every-fourth-dot` | `vdc/chip.rs` | the CPU's VRAM slots repeat every 4 dots, not 8 (U1) | rung 5 (`sock-path-rtype`) |
| `raster-compare-a-line-early` | `vdc/chip.rs` | the raster compare fires a line early | rung 2 (`base-probe`) |
| `seventeen-sprites` | `vdc/sprites.rs` | a line holds 17 sprite entries before the overflow | rung 4 (`lib-Super Star Soldier (USA)`) |
| `sprites-always-in-front` | `vdc/sprites.rs` | a sprite pixel hides the background whatever its priority bit | rung 2 (`base-probe`) |
| `bat-height-from-the-width-bit` | `vdc/background.rs` | the BAT's height follows MWR D5 | rung 2 (`base-probe`) |
| `vdw-eight-bits` | `vdc/regs.rs` | the display height register is 8 bits wide | rung 2 (`base-probe`) |
| `display-start-a-character-late` | `vdc/sequencer.rs` | the display starts HDS + 2 characters after the line step, not HDS + 1 | rung 2 (`covell`) |
| `scroll-latched-a-character-early` | `vdc/sequencer.rs` | the scroll is taken 24 dots before the display start, not 16 (the closed-box fit) | rung 2 (`base-probe`) |
| `byr-reloads-on-the-msb-only` | `vdc/chip.rs` | a write of BYR's low byte alone does not reload the vertical scroll counter | rung 2 (`base-probe`) |
| `scroll-counter-not-stepped` | `vdc/chip.rs` | the vertical scroll counter does not count the line steps | rung 2 (`covell`) |
| `vds-a-line-short` | `vdc/sequencer.rs` | the display period starts a line early | rung 2 (`covell`) |
| `front-porch-vcr-plus-two` | `vdc/sequencer.rs` | the front porch is VCR + 2 lines, not pcetech's VCR + 3 | rung 5 (`sw-psg-mame-aero`) |
| `raster-counter-one-at-the-display` | `vdc/sequencer.rs` | the raster counter is $41, not $40, on the line before the display period | rung 2 (`base-probe`) |
| `burst-from-the-background-bit` | `vdc/sequencer.rs` | burst mode is decided from the background enable alone | rung 5 (`mach-tg16-gradius`) |
| `cut-display-loses-its-vblank` | `vdc/sequencer.rs` | a display period the vertical sync cuts short has no vertical blanking (C27) | rung 2 (`base-probe`) |
| `sat-transfer-two-lines` | `vdc/dma.rs` | the SAT transfer is busy two lines, not pcetech's three | rung 2 (`base-probe`) |
| `dma-cancelled-by-the-display` | `vdc/dma.rs` | a VRAM DMA running at the display start is cancelled whatever `dma_resumes` says (C7) | rung 2 (`base-probe`) |
| `vram-dma-one-dot-a-word` | `vdc/dma.rs` | a VRAM DMA access takes one dot whatever MWR's access width | rung 4 (`lib-Dungeons & Dragons - Order of the Griffon (USA)`) |
| `sprite-fetch-budget-ignored` | `vdc/chip.rs` | the manual's pattern-fetch budget (2.4.1 (6)) is not applied | rung 4 (`lib-Yo' Bro (USA)`) |
| `sprite-search-budget-ignored` | `vdc/chip.rs` | the manual's SAT search budget (2.4.1 (6)) is not applied | rung 4 (smoke) (`lib-Galaga '90 (USA)`) |
| `mwr-fields-live` | `vdc/chip.rs` | the sprite scan reads MWR's SM live, not as the last vertical blanking took it | `cargo test` (`the_sprite_access_width_waits_for_the_vertical_blanking`) |
| `vwr-write-not-stopped-for-the-wait` | `vdc/chip.rs` | a VWR write while one is pending does not stop for the board's wait (issue 0004) | rung 4 (`lib-Puzznic (Japan) (En)`) |
| `noise-frequency-not-complemented` | `psg/chip.rs` | the noise period is taken from NF, not its complement | rung 2 (`base-probe`) |
| `noise-feedback-bit` | `psg/chip.rs` | the noise shift register is 17 bits long | rung 2 (`base-probe`) |
| `lfo-shift` | `psg/chip.rs` | the LFO's modulation is shifted by 0, 1, 2 where a period is first loaded | rung 2 (`base-probe`) |
| `key-on-steps-the-address` | `psg/chip.rs` | key-on does what the other `keyon_mode` says (C22) | rung 2 (`base-probe`) |
| `dda-latch-needs-on` | `psg/chip.rs` | R6 in DDA mode loads the latch only with the channel on (C16, Mednafen's way) | rung 2 (`base-probe`) |
| `mixer-rounds-half-away` | `mixer.rs` | the mixer rounds a half away from zero (Rust's `round`), not to even (Python's) | rung 5 (`sock-port-lazers`) |
| `mixer-hold-rounds` | `mixer.rs` | a source at another rate is held by a rounded count | rung 5 (`sock-port-lazers`) |
| `mixer-gain-twice` | `mixer.rs` | a source that does not apply its own gain gets it doubled | rung 5 (`sock-port-lazers`) |
| `hsync-a-clock-short` | `machine.rs` | the VCE's horizontal sync is 236 master clocks, not 237 | rung 2 (`covell`) |
| `irq-line-is-a-single-flag` | `sockets.rs` | a source letting go of an interrupt line lowers it for every source | `cargo test` (`irq_line_is_a_wired_or_of_sources_never_a_max`) |
| `work-ram-half-size` | `pagetable.rs` | work RAM mirrors every 4 KiB | rung 2 (`covell`) |
| `interrupt-registers-unmapped` | `hardware_page.rs` | $FF:1400-17FF answers $FF | rung 2 (`base-probe`) |
| `state-forgets-the-capture` | `state.rs` | a loaded state keeps the picture rows of the system it is loaded into | `cargo test` (`a_loaded_run_continues_bus_cycle_for_bus_cycle`) |
| `state-drops-the-polled-lines` | `state.rs` | a state does not carry the lines the CPU's last poll saw (huc6280-python 0.4.0) | `cargo test` (`a_state_carries_the_lines_the_last_poll_saw`) |
| `state-forgets-the-pad-latch` | `state.rs` | a loaded state keeps the SEL/CLR latch of the system it is loaded into | rung 6 (`pic-Aero Blasters (USA)`) |
| `state-takes-the-timer-switch` | `state.rs` | a loaded state overwrites the system's own `underflow_window_mclk` | `cargo test` (`loading_keeps_the_wiring_and_the_switches`) |
| `no-reset-requested` | `system.rs` | the CPU is put on the board without a reset: no start sequence, no reset vector | rung 2 (`covell`) |
| `seed-low-bits` | `mt.rs` | a seeded power-on takes the low bits of each random word | rung 5 (`sock-path-rtype`) |

**63 caught, 3 missed**: 34 by rung 2 (Covell's test or the probe card), 13
by rung 5, 2 by the smoke list, 9 by the unit tests, 4 by the whole library,
1 by rung 6. Two unit tests were added for faults the broken test stage had
hidden: `a_state_carries_the_lines_the_last_poll_saw` (tests/state.rs) and
`the_sprite_access_width_waits_for_the_vertical_blanking` (tests/vdc_wait.rs);
a third, `a_waited_vram_write_is_made_where_the_wait_ended`, pins the wait's
time but does not catch `served-access-made-at-the-old-time`.

**The three missed are the board's wait (the reference's issue 0004), not the
rewrite**, and were missed in 0.2.0 too (hidden by the test stage):

- `served-access-made-at-the-old-time`: probably an equivalent fault. The
  access made again at the old time finds nothing pending (the wait served
  it), so it is accepted at once, and the commit time it computes is a CPU
  slot at or before the time the wait ended, so it commits at once too; the
  CPU's hold was already applied. No case or test found a difference.
- `wait-not-settled`: the devices are caught up at the next advance anyway;
  a difference needs a device read between the wait's end and that advance.
- `accesses-after-a-stop-made`: needs a decode that touches a second VDC
  after the first one waits (the lockstep pixel path on `ST0`-`ST2` with a
  VWR write pending on both), which no case reaches.

What would settle them: a unit test of a two-VDC decode with both VDCs busy,
and one that reads a port device's state right after a wait; or a probe-card
section that makes those accesses.

What the 0.2.0 record said (its five "caught by `cargo test` (a test the script could not name)"
rows were the broken test stage; those faults never went on to the later checks, so its true count
is not known. Run on 2026-11-19 with the stage fixed: `state-forgets-the-pad-latch` is caught by
rung 6, `state-drops-the-polled-lines` by the new test, and the three wait faults above are
missed):

**56 caught, 0 missed**: 30 by rung 2 (Covell's test or the probe card), 12
by rung 5, 1 by the smoke list, 13 by the unit tests. Rung 3, the whole
library and rung 6 come late in the order and were never needed. The probe
card's waited-on DMA (`base-probe`) catches three of the six wait faults; the
other three (where the served access is made, the settle after the wait, the
accesses after a stop) are caught by the unit tests (`tests/vdc_wait.rs` pins
the reference's two wait tests and the two-VDC decode).

What the 0.1.0 record said:

**48 caught, 0 missed**: 28 by rung 2 (Covell's test or the probe card), 13
by rung 5, 1 by the smoke list, 5 by the unit tests (`tests/sockets.rs` and
`tests/pads.rs`, which rung 5 runs, and `tests/state.rs`, which rung 6
runs), 1 by rung 6's round trip on the titles. Rung 3 and the whole library
come late in the order and were never needed.

The first run of this list missed five. Three were holes in the cases, not
in the board: the probe card never wrote a scroll register on the dot it is
latched (`byr-latched-a-dot-late`), never set a display height past 8 bits
(`vdw-eight-bits`) and never keyed a channel on under the LFO (`lfo-shift`).
It does all three now. Two changed no behaviour, which makes them not
faults, and were dropped: the PSG's sample boundary tested with `>` where
the reference has `>=` (they differ only when a float sum lands exactly on
the tolerance), and `System::new` without its forced first delivery of the
interrupt lines (all three are low at power-on, which is what the CPU
already holds). `pad6-counts-clr-level` is caught by a unit test only: on
every pad scan the cases make, the faulty pad's count has the right parity.

## 0.2.0 (2026-11-13): 76 caught, 0 missed

After the merge of pcengine-rust 0.2.0, against the rebuilt reference cache: the base board's 56 (its
48 and the eight new ones on the VDC wait, the PSG's level table and the CPU state's
`polled_irq_lines`; pcengine-rust's docs/seeded-faults.md has their table) and this fork's twenty
below, **76 caught, 0 missed**. The new eight are caught by the same checks as on the base board
(`vdc-waits-alone` and `wait-not-held` by rung 2's `base-probe`, the other six by the unit tests);
`wait-not-held` names the fork's line (`self.held += served - mclk`: the fork adds the served wait's
hold directly, not through `Board::hold`, which also calls `touch`). Every fault of the 0.1.0 run is
caught by the same check as then.

## This fork's shortcuts (seventeen since 2026-11-19)

2026-11-19: all seventeen caught. Three of the twenty went with the clean rewrite:
`line-events-not-emptied` (its text was in the withdrawn sequencer; cut unread),
`sprite-list-not-emptied` (`scan_into` not kept) and `row-buffer-not-cleared` (the reuse inert
after the merge, taken out); docs/not-kept.md. Two are now caught earlier than before:
`next-event-forgets-the-commit` by rung 5 (`sock-path-rtype`) and `full-read-skips-the-events`
by rung 2 (`covell`). The text below is the 2026-11-12 record.


pcengine-rust-fast keeps the 48 above, each still caught by the same check, and adds at least
one fault on every kept change that has code: **68 caught, 0 missed** (2026-11-12, and re-run
on 2026-11-13 with the CPU core swapped for huc6280-rust-fast: the same). The unit tests of `tests/fork.rs` catch those that change the state but no
bus cycle, picture or sample. The first run missed one, `next-event-forgets-the-commit`: at 12
master clocks a cycle a VRAM write's slot always falls inside the access's own stretch, so the
test program now runs at high CPU speed. The faults are built without LTO (the same source).

| Fault | Module | What it changes | Caught by |
| --- | --- | --- | --- |
| `horizon-waits-for-the-underflow` | `board.rs` | the event horizon stops for the timer's underflow only, so the count is behind the clock between | `cargo test` (`every_device_is_caught_up_after_every_instruction`) |
| `horizon-sleeps-through-a-dma` | `board.rs` | the event horizon does not stop for a running VRAM DMA: its words move late | `cargo test` (`a_port_device_that_reaches_a_vdc_from_catch_up_moves_the_horizon`) |
| `access-keeps-the-old-horizon` | `board.rs` | what an access schedules (a VRAM commit, the timer's start) is not in the event horizon | `cargo test` (`every_device_is_caught_up_after_every_instruction`) |
| `next-event-forgets-the-commit` | `vdc/chip.rs` | a VDC's next event leaves out a waiting VRAM access's slot | `cargo test` (`every_device_is_caught_up_after_every_instruction`) |
| `any-card-is-plain` | `board.rs` | a card that is not this crate's own (it may hold the CPU, raise IRQ2) has its accesses cut short too | rung 5 (`sock-card-bonk`) |
| `quiet-access-with-a-hold-waiting` | `board.rs` | a work RAM or card access is cut short with a hold from outside waiting | `cargo test` (`a_hold_from_a_port_devices_catch_up_stretches_that_ram_access`) |
| `palette-write-waits-for-the-row-end` | `video.rs` | the capture's quick row is taken with palette writes inside the row: they show from the next row | rung 2 (`covell`) |
| `tile-line-mirrored` | `vdc/background.rs` | the bitplane table puts a tile line's pixels in the other order | rung 2 (`covell`) |
| `background-fine-scroll-dropped` | `vdc/background.rs` | the first tile of a background row is drawn from its left edge whatever BXR's low bits | rung 2 (`base-probe`) |
| `any-path-is-direct` | `board.rs` | the capture reads the first VDC's row whatever pixel path is plugged in | rung 5 (`sock-path-rtype`) |
| `port-catch-up-keeps-the-old-horizon` | `board.rs` | what a port device's `catch_up` does to a VDC is not in the event horizon | `cargo test` (`a_port_device_that_reaches_a_vdc_from_catch_up_moves_the_horizon`) |
| `quiet-idle-cycle-not-tapped` | `board.rs` | an idle cycle before the event horizon is left out of the bus log | rung 2 (`covell`) |
| `full-read-skips-the-events` | `board.rs` | a read cycle that reaches the event horizon does not run the events | rung 2 (`base-probe`) |
| `rom-lent-beside-a-port-device` | `board.rs` | a RomCard's image is taken by the board although a port device could look at the card | `cargo test` (`a_rom_card_has_its_image_whenever_a_port_device_looks`) |
| `rom-not-given-back` | `board.rs` | a RomCard's image stays with the board when the host reaches the card | `cargo test` (`a_rom_card_has_its_image_whenever_the_host_looks`) |
| `lent-rom-bank-offsets` | `cards.rs` | the board reads a lent image's banks from the wrong place | rung 2 (`covell`) |
| `hold-from-catch-up-waits` | `board.rs` | a hold a port device leaves from `catch_up` is not applied by a work RAM or plain card access | `cargo test` (`a_hold_from_a_port_devices_catch_up_stretches_that_ram_access`) |
