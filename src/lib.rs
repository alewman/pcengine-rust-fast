//! A Rust PC Engine / TurboGrafx-16 board transcribed from
//! [pcengine-python](https://github.com/alewman/pcengine-python) at `v0.1.0`
//! (`bc9b976`) and proven equivalent to it: every CPU bus cycle, every frame's
//! picture at the 9-bit palette value, every mixed audio sample.
//!
//! **This is the speed fork** (package `pcengine-rust-fast`, the same crate name and public
//! API as pcengine-rust): the same board with shortcuts that are kept only while its traces
//! stay identical to the base board's and a measurement shows they pay (README). A host
//! reaches the board through `System` (`board_mut`, `step`, `run_frame`, `run_until`); one
//! that drives a bare `Board` and changes it between bus callbacks calls `Board::touch`.
//!
//! Each module is the transcription of one `pcengine_python` module, with the
//! same names, every named switch as a field with the reference's default, and
//! the comments that cite a source carried over. Where Rust forced a
//! difference, the module header says so.
//!
//! ```
//! use pcengine_rust::board::{Board, BoardOptions};
//! use pcengine_rust::cards::make_card;
//! use pcengine_rust::pads::{Pad2, RUN};
//! use pcengine_rust::system::System;
//!
//! let image = vec![0u8; 0x2000]; // a HuCard image (bring your own)
//! let board = Board::new(BoardOptions {
//!     card: Some(make_card(&image).unwrap()),
//!     pad: Some(Box::new(Pad2::new())),
//!     ..BoardOptions::default()
//! })
//! .unwrap();
//! let mut system = System::new(board);
//! system.board_mut().set_pressed(0, RUN);
//! system.run_frame();
//! let _samples = system.board_mut().take_audio(); // 44.1 kHz stereo
//! let _picture = system.board().capture.last_frame.as_ref(); // 242 rows of 9-bit values
//! ```

pub mod board;
pub mod cards;
pub mod cpu_io;
pub mod fake_devices;
pub mod hardware_page;
pub mod machine;
pub mod mixer;
pub mod mt;
pub mod pads;
pub mod pagetable;
pub mod psg;
pub mod sockets;
pub mod state;
pub mod system;
pub mod trace;
pub mod vce;
pub mod vdc;
pub mod video;

pub use huc6280;
