<!-- SPDX-License-Identifier: MIT -->
# Third-party notices

pcengine-rust-fast, like pcengine-rust, the board it is a speed fork of, is MIT licensed
([LICENSE](LICENSE)). It contains no code from the GPL and LGPL
emulators its comments cite (Mednafen, Beetle PCE, Mesen2, Geargrafx, the MiSTer core, parts of
MAME): the only sources transcribed are pcengine-python and huc6280-python, our own MIT
repositories, which name those emulators as referees and readings, not as sources of code
(pcengine-python's [docs/provenance.md](https://github.com/alewman/pcengine-python/blob/main/docs/provenance.md)).

Since the clean rewrite of 2026-11-19 ([docs/restart.md](docs/restart.md)) the code that followed
GPL sources in the reference, and its Rust counterpart here, has been withdrawn unread and
transcribed again from the reference's replacement; those emulators are run as closed boxes only.

Permissively licensed material is acknowledged here, as pcengine-python's NOTICE.md acknowledges
it:

- **MAME's HuC6270 and HuC6260 devices**, `src/devices/video/huc6270.cpp` and `huc6260.cpp`,
  BSD-3-Clause, copyright Wilbert Pol. The reference read them as a description of the chips where
  Hudson's manuals and MacDonald's notes are silent; the behaviour they describe informs the
  reference's `vdc/sequencer.py`, `vdc/chip.py` (the vertical scroll counter, BYR on either byte)
  and the constants `HSYNC_CLOCKS` and `LINE_CLOCKS` in `machine.py`, and so this crate's
  `src/vdc/sequencer.rs`, `src/vdc/chip.rs` and `src/machine.rs`, which transcribe them and name
  this source in their headers or comments. No MAME code was transcribed, there or here. The
  BSD-3-Clause notice below applies to anything taken from them.
- **MAME's PC Engine cartridge devices**, `src/devices/bus/pce/pce_slot.cpp` and `pce_rom.cpp`
  (the `pce_sf2`, `pce_populous` and `pce_tennokoe` boards), BSD-3-Clause, copyright Fabio Priuli.
  Read by the reference as a description of the cards: the mirroring of odd-sized images, the
  Street Fighter II mapper's decode, Populous's RAM window and the Tennokoe Bank's lock inform its
  `cards.py` and so `src/cards.rs`. No code was transcribed. The BSD-3-Clause notice below applies
  to anything taken from them.

One small table has a permissively licensed origin:

- `src/psg/chip.rs`, the sixteen values of the optional `scale_table_quirk` volume table
  (transcribed from pcengine-python's `psg/chip.py`), are those of MAME's
  `src/devices/sound/c6280.cpp`, which is BSD-3-Clause, copyright Charles MacDonald.
  Redistribution of that file's material requires its notice:
  "Redistribution and use in source and binary forms, with or without modification, are permitted
  provided that the following conditions are met: 1. Redistributions of source code must retain
  the above copyright notice, this list of conditions and the following disclaimer. 2.
  Redistributions in binary form must reproduce the above copyright notice, this list of
  conditions and the following disclaimer in the documentation and/or other materials provided
  with the distribution. 3. Neither the name of the copyright holder nor the names of its
  contributors may be used to endorse or promote products derived from this software without
  specific prior written permission. THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND
  CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
  IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO
  EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
  SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
  CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
  NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF
  THE POSSIBILITY OF SUCH DAMAGE."
