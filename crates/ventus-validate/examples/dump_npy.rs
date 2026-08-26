//! Emits `.npy` fixtures plus an exact specification of what they contain, so
//! that `analysis/verify_npy.py` can rebuild the same arrays with numpy and
//! compare byte for byte.
//!
//! Data are written to the spec file as raw IEEE-754 bit patterns, so nothing
//! is lost to decimal formatting on either side.
//!
//!   cargo run -p ventus-validate --example dump_npy -- <out_dir>

use std::fmt::Write as _;
use std::path::PathBuf;
use ventus_validate::npy;

fn main() {
    let dir: PathBuf = std::env::args()
        .nth(1)
        .expect("usage: dump_npy <out_dir>")
        .into();
    std::fs::create_dir_all(&dir).expect("create out dir");

    let mut fixtures: Vec<(String, Vec<usize>, Vec<f64>)> = vec![
        ("scalar".into(), vec![], vec![42.0]),
        ("vec3".into(), vec![3], vec![1.0, -2.5, 3.25]),
        ("mat2x3".into(), vec![2, 3], (0..6).map(f64::from).collect()),
        ("empty".into(), vec![0], vec![]),
        (
            "specials".into(),
            vec![5],
            vec![0.0, -0.0, f64::INFINITY, f64::NEG_INFINITY, f64::NAN],
        ),
        (
            "design_point".into(),
            vec![6],
            vec![220.65, 2930.4, 0.046266, 297.78, 893.34, 612.0],
        ),
        (
            "long1d".into(),
            vec![1000],
            (0..1000).map(|i| i as f64 * 0.5).collect(),
        ),
        ("wide1d".into(), vec![123_456], vec![1.5; 123_456]),
    ];

    // A shape whose header is already 64-byte aligned before padding, where
    // numpy adds a whole extra block. Reached by dimension count, not by digits.
    for k in 2..80_usize {
        let shape = vec![1_usize; k];
        let inner: Vec<String> = shape.iter().map(usize::to_string).collect();
        let dict_len = format!(
            "{{'descr': '<f8', 'fortran_order': False, 'shape': ({}), }}",
            inner.join(", ")
        )
        .len();
        if (10 + dict_len + 1) % 64 == 0 {
            fixtures.push((format!("aligned_k{k}"), shape, vec![7.0]));
            break;
        }
    }

    let mut spec = String::new();
    for (name, shape, data) in &fixtures {
        let path = dir.join(format!("{name}.npy"));
        npy::write_f64(&path, shape, data).expect("write npy");

        let shape_txt: Vec<String> = shape.iter().map(usize::to_string).collect();
        let data_txt: Vec<String> = data
            .iter()
            .map(|v| format!("{:016x}", v.to_bits()))
            .collect();
        let _ = writeln!(
            spec,
            "{}\t{}\t{}",
            name,
            shape_txt.join(","),
            data_txt.join(",")
        );
    }
    std::fs::write(dir.join("spec.tsv"), spec).expect("write spec");
    println!("wrote {} fixtures to {}", fixtures.len(), dir.display());
}
