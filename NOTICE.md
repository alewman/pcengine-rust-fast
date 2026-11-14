# Third-party notices

pcengine-rust is MIT licensed ([LICENSE](LICENSE)). It contains no code from the GPL and LGPL
emulators its comments cite (Mednafen, Beetle PCE, Mesen2, Geargrafx, the MiSTer core, parts of
MAME): the only sources transcribed are pcengine-python and huc6280-python, our own MIT
repositories, which name those emulators as referees and readings, not as sources of code
(pcengine-python's [docs/provenance.md](https://github.com/alewman/pcengine-python/blob/main/docs/provenance.md)).

One small table has a permissively licensed origin and is acknowledged here, as pcengine-python's
NOTICE.md acknowledges it:

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
