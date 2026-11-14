# Seeded faults (rung 7)

A gate that cannot fail proves nothing. This is the record of the run that certified the
board (2026-11-12, `scripts/rung7.sh` at `49e3d48`). The two later commits (`132e548` and
`9c4608f`: the PSG's level curve as a written-out table) change no line a fault is planted in.

`scripts/seeded_faults.py` plants one single-line fault at a time in a copy
of `src/`, builds it, and runs the checks cheapest first until one fails:
rung 2 (Covell's test and the probe card), rung 5 (the socket, machine and
switch cases), the smoke list of rung 4, `cargo test`, rung 3, the whole
library, rung 6.

| Fault | Module | What it changes | Caught by |
| --- | --- | --- | --- |
| `vce-access-not-stretched` | `board.rs` | only a VDC access takes the extra cycle, not a VCE one (C4, ares' reading) | rung 2 (`covell`) |
| `row-ends-a-dot-late` | `board.rs` | a line's picture row is handed to the capture 4 master clocks late | `cargo test` (`a_replacement_pixel_path_sees_every_line`) |
| `irq1-from-the-first-vdc-only` | `board.rs` | a second VDC's interrupt is not ORed into IRQ1 (the reference's issue 0001) | rung 5 (`sock-path-probe`) |
| `port-device-not-caught-up` | `board.rs` | the board never runs the port device up to its clock (the reference's issue 0002) | rung 5 (`sock-port-lazers`) |
| `disable-mask-not-delivered` | `board.rs` | a write to $1402 never reaches the CPU | rung 5 (`sock-multitap-bomberman94`) |
| `lines-delivered-only-on-advance` | `board.rs` | an interrupt line an access changed is seen by the CPU a cycle later | rung 2 (`base-probe`) |
| `timer-prescaler-not-reset` | `cpu_io/timer.rs` | the first tick after an enable comes a CPU cycle early (U24) | rung 2 (`base-probe`) |
| `timer-window-a-clock-long` | `cpu_io/timer.rs` | the count reads $7F one master clock longer after an underflow (C25) | rung 2 (`base-probe`) |
| `irq-status-buffer-bits` | `cpu_io/irq_ctl.rs` | $1402/$1403 return only four bits of the I/O buffer | rung 2 (`base-probe`) |
| `io-port-d4-low` | `cpu_io/io_port.rs` | the I/O port's D4 reads 0 | rung 2 (`covell`) |
| `read-does-not-load-the-buffer` | `cpu_io/mod.rs` | a read of the timer, the port or the interrupt registers leaves the I/O buffer | rung 2 (`base-probe`) |
| `pad6-counts-clr-level` | `pads.rs` | the 6-button pad changes scan on every write with CLR high, not on its rise | `cargo test` (`six_button_ignores_repeated_clr_high`) |
| `multitap-counts-under-clr` | `pads.rs` | the multitap's counter steps on a SEL rise while CLR is high | rung 5 (`sock-multitap-probe`) |
| `card-384k-upper-mirror` | `cards.rs` | a 384 KiB card's banks $40-$7F show the wrong part of the image | rung 5 (`sock-card-bonk`) |
| `tennokoe-lock-inverted` | `cards.rs` | the Tennokoe card's RAM unlocks on a zero | rung 5 (`sock-tennokoe-probe`) |
| `populous-ram-two-banks` | `cards.rs` | the Populous card's RAM takes writes in two banks of its four | rung 4, smoke (`lib-Populous (Japan) (En)`) |
| `palette-lsb-clears-bit-8` | `vce.rs` | a write of a colour's low byte clears its ninth bit | rung 2 (`covell`) |
| `palette-write-a-dot-late` | `video.rs` | a palette write made exactly at a dot's start shows from the next dot | rung 2 (`covell`) |
| `line-count-ignores-the-machine-row` | `video.rs` | `lines_follow_vce_bit2 = false` still follows VCE bit 2 (C3) | rung 5 (`sw-no-bit2-lines-probe`) |
| `cpu-slot-every-fourth-dot` | `vdc/chip.rs` | the CPU's VRAM slots repeat every 4 dots, not 8 (U1) | rung 5 (`sock-path-rtype`) |
| `raster-compare-a-line-early` | `vdc/chip.rs` | the raster compare fires a line early | rung 2 (`base-probe`) |
| `byr-set-on-the-msb-only` | `vdc/chip.rs` | a write of BYR's low byte alone does not move the line counter (Mednafen's note) | rung 2 (`base-probe`) |
| `byr-latched-a-dot-late` | `vdc/sequencer.rs` | BYR is latched a dot later in the line (U3) | rung 2 (`base-probe`) |
| `vds-a-line-short` | `vdc/sequencer.rs` | the display period starts a line early | rung 2 (`covell`) |
| `burst-from-the-background-bit` | `vdc/sequencer.rs` | burst mode is decided from the background enable alone | rung 5 (`mach-tg16-gradius`) |
| `sat-transfer-time-fixed` | `vdc/dma.rs` | the SAT transfer takes 1024 dots of the 5 MHz clock whatever the dot clock | rung 2 (`base-probe`) |
| `dma-cancelled-by-the-display` | `vdc/dma.rs` | a VRAM DMA running when the display starts is cancelled whatever `dma_resumes` says (C7) | rung 2 (`base-probe`) |
| `seventeen-sprites` | `vdc/sprites.rs` | a line holds 17 sprite entries before the overflow | rung 2 (`base-probe`) |
| `sprites-always-in-front` | `vdc/sprites.rs` | a sprite pixel hides the background whatever its priority bit | rung 2 (`base-probe`) |
| `bat-height-from-the-width-bit` | `vdc/background.rs` | the BAT's height follows MWR D5 | rung 2 (`base-probe`) |
| `vdw-eight-bits` | `vdc/regs.rs` | the display height register is 8 bits wide | rung 2 (`base-probe`) |
| `noise-frequency-not-complemented` | `psg/chip.rs` | the noise period is taken from NF, not its complement | rung 2 (`base-probe`) |
| `noise-feedback-bit` | `psg/chip.rs` | the noise shift register is 17 bits long | rung 2 (`base-probe`) |
| `lfo-shift` | `psg/chip.rs` | the LFO's modulation is shifted by 0, 1, 2 where a period is first loaded | rung 2 (`base-probe`) |
| `key-on-steps-the-address` | `psg/chip.rs` | key-on does what the other `keyon_mode` says (C22) | rung 2 (`base-probe`) |
| `dda-latch-needs-on` | `psg/chip.rs` | R6 in DDA mode loads the latch only with the channel on (C16, Mednafen's way) | rung 2 (`base-probe`) |
| `mixer-rounds-half-away` | `mixer.rs` | the mixer rounds a half away from zero (Rust's `round`), not to even (Python's) | rung 5 (`sock-port-lazers`) |
| `mixer-hold-rounds` | `mixer.rs` | a source at another rate is held by a rounded count | rung 5 (`sock-port-lazers`) |
| `mixer-gain-twice` | `mixer.rs` | a source that does not apply its own gain gets it doubled | rung 5 (`sock-port-lazers`) |
| `hsync-a-clock-short` | `machine.rs` | the horizontal sync is 236 master clocks | rung 2 (`covell`) |
| `irq-line-is-a-single-flag` | `sockets.rs` | a source letting go of an interrupt line lowers it for every source | `cargo test` (`irq_line_is_a_wired_or_of_sources_never_a_max`) |
| `work-ram-half-size` | `pagetable.rs` | work RAM mirrors every 4 KiB | rung 2 (`covell`) |
| `interrupt-registers-unmapped` | `hardware_page.rs` | $FF:1400-17FF answers $FF | rung 2 (`base-probe`) |
| `state-forgets-the-capture` | `state.rs` | a loaded state keeps the picture rows of the system it is loaded into | `cargo test` (`a_loaded_run_continues_bus_cycle_for_bus_cycle`) |
| `state-forgets-the-pad-latch` | `state.rs` | a loaded state keeps the SEL/CLR latch of the system it is loaded into | rung 6 (`pic-Aero Blasters (USA)`) |
| `state-takes-the-timer-switch` | `state.rs` | a loaded state overwrites the system's own `underflow_window_mclk` | `cargo test` (`loading_keeps_the_wiring_and_the_switches`) |
| `no-reset-requested` | `system.rs` | the CPU is put on the board without a reset: no start sequence, no reset vector | rung 2 (`covell`) |
| `seed-low-bits` | `mt.rs` | a seeded power-on takes the low bits of each random word | rung 5 (`sock-path-rtype`) |

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
