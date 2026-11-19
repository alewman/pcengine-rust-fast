// SPDX-License-Identifier: MIT
//! The I/O buffer: the last value read from or written to $0800-$17FF (`cpu_io/io_buffer.py`).
//!
//! [hw: pcetech 3 note 1] Reads of some locations return buffer bits where the manual says
//! "unused"; games that read them see the buffer.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IoBuffer {
    pub value: u8,
}

impl IoBuffer {
    pub fn new(value: u8) -> IoBuffer {
        IoBuffer { value }
    }

    pub fn reset(&mut self) {
        self.value = 0;
    }
}
