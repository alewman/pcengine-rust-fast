//! The PSG's level curve is a written-out table (so every target has the same doubles); on the
//! targets where the reference was run, the platform's `powf` gives the same values.

use pcengine_rust::psg::chip::LEVEL_BITS;

#[test]
fn the_table_is_ten_to_the_minus_three_fortieths_per_unit() {
    for (u, bits) in LEVEL_BITS.iter().enumerate() {
        let level = f64::from_bits(*bits);
        let wanted = 10f64.powf(-1.5 * u as f64 / 20.0);
        // Within one unit in the last place on any platform ...
        assert!((level - wanted).abs() <= wanted * f64::EPSILON, "unit {u}");
        // ... and exactly the platform's value where the lockstep gate ran (x86-64 Linux, glibc).
        if cfg!(all(
            target_arch = "x86_64",
            target_os = "linux",
            target_env = "gnu"
        )) {
            assert_eq!(level.to_bits(), wanted.to_bits(), "unit {u}");
        }
    }
    assert_eq!(f64::from_bits(LEVEL_BITS[0]), 1.0);
}
