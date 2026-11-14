# Tried and not kept

Each with its A/B (scripts/ab.py: two builds alternated on one pinned core, the best of 12 runs a
side on each of five titles, CPU time). An A/A run of that measurement is within 1.5% overall
(x1.004, x0.983 and x0.987 were seen) and 6% a title. And a build has its own luck: without
LTO, how the compiler splits the crate into codegen units moved the same change from x1.09 to
x1.01 between two builds of it, so a small factor needs LTO builds on both sides, and one
below about 3% cannot be told from nothing here.

- **Sprites from the bitplane table.** `sprites::draw` with the line buffer on the stack, a
  pattern row's sixteen pixels from the table the background uses (a 64-bit word of nibbles), an
  all-transparent row skipped and only the columns between the leftmost and the rightmost
  sprite merged into the line. x1.007 against the build before it, and x1.004 when taken out
  of a later build again (Street Fighter II', the sprite-heavy title, x0.97 and x1.00): noise.
  The gate was green and two seeded faults on it were caught.
- **Sprites again, at the end**, when they had grown to 8% of Blazing Lazers and 13% of Street
  Fighter II': as above, but only the columns under an entry cleared and merged (a merge done
  twice, where two entries overlap, changes nothing more), so a line with two sprites at its
  two ends no longer merges the whole width. x1.008 (Street Fighter II' x1.021, R-Type x0.986),
  with the gate green: noise again. What costs in the sprites is not the merge or the bit
  shuffling but the scan of the 64 entries on every line and the per-pixel tests, which stay.
- **The capture's colour table by pixel value.** The quick row of `Capture::finish_line` tests
  every pixel's colour for 0 to pick the backdrop or overscan entry; a 512-entry table kept by
  pixel value, with that resolved and rebuilt after a row with palette writes, makes a pixel
  one lookup. x1.026 and x1.028 when taken out of the last build and put back (two runs), but
  x0.990 against the build before it in the chain: not a gain the measurement can tell from
  its noise, and it is a derived cache in the capture with an equality written by hand to
  ignore it. Not in the tree.
- **Thin LTO** in place of fat LTO with one codegen unit: the fat build is x1.107 over it.
- **One codegen unit without LTO**: the fat LTO build is x1.097 over it. What pays is inlining
  across the crate borders (the CPU core, the 65xx frame under it), not the unit count.

## Not attempted, and why

- **A lent image for the mapper cards** (`Sf2Card`, `PopulousCard`, `TennokoeCard`): the board
  would have to keep a second copy of the mapper's state (the SF2 page, the RAM lock) in step
  with the card's. They are three cards; they take the plain-card path (no `finish_access`), one
  call through the trait an access.
- **A pointer into the card's image** instead of the loan: `slot.card` and `RomCard::data` are
  public, so a pointer the board kept could dangle in safe code. Not done; the loan moves the
  `Vec` and is safe.
- **Skipping the horizon's recomputation after a port device's `catch_up`** (it is a tenth of a
  cycle in that configuration): the device is handed the VDCs through its pins and may change
  their schedule, and the board cannot tell cheaply that it did not.
- **The CPU core.** After these changes huc6280-rust and the 65xx frame under it are the largest
  share of a frame (README, "Where the time goes"). The brief says to report it, not to edit it.
  Done since in its own speed fork, huc6280-rust-fast, which this board now uses (x1.075 on the
  five titles, 2026-11-13; README, "Speed").
