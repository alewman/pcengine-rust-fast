//! The native side of the wasm comparison.
//!
//!     pce-wasm-bench                         # the 128 level-table entries as hex bits
//!     pce-wasm-bench IMAGE [tg16] FRAMES     # the two checksums of the run bench.mjs makes

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        for u in 0..128 {
            println!("{:016x}", pce_wasm_bench::level(u).to_bits());
        }
        return;
    }
    let image = std::fs::read(&args[0]).expect("the image");
    let tg16 = args.iter().any(|arg| arg == "tg16");
    let frames = args.last().unwrap().parse().expect("FRAMES");
    let mut run = pce_wasm_bench::Run::new(&image, tg16);
    run.run(frames);
    println!(
        "output {:08X}, check {:08X}",
        run.output_check(),
        run.check()
    );
}
