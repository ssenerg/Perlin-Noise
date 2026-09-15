use perlin_noise::Instance;

/// Row-major index of a lattice point in a table of the given shape.
fn flat(shape: &[usize], index: &[usize]) -> usize {
    index
        .iter()
        .zip(shape)
        .fold(0, |acc, (coord, size)| acc * size + coord)
}

#[test]
fn rejects_invalid_dimensions() {
    assert!(Instance::new(vec![], 0).is_err());
    assert!(Instance::new(vec![3, 0, 2], 0).is_err());
    assert!(Instance::new(vec![2; perlin_noise::MAX_DIMS + 1], 0).is_err());
}

#[test]
fn lattice_has_one_unit_gradient_per_intersection() {
    let noise = Instance::new(vec![3, 1, 2], 7).unwrap();
    assert_eq!(noise.shape(), &[4, 2, 3]);
    assert_eq!(noise.intersections(), 4 * 2 * 3);

    for x in 0..4 {
        for y in 0..2 {
            for z in 0..3 {
                let gradient = noise.gradient(&[x, y, z]).expect("inside the lattice");
                let norm = gradient.iter().map(|v| v * v).sum::<f64>().sqrt();
                assert!((norm - 1.0).abs() < 1e-12, "gradient norm was {norm}");
            }
        }
    }
    assert!(noise.gradient(&[4, 0, 0]).is_none());
    assert!(noise.gradient(&[0, 0]).is_none());
}

#[test]
fn same_seed_gives_the_same_field() {
    let a = Instance::new(vec![4, 3], 12345).unwrap();
    let b = Instance::new(vec![4, 3], 12345).unwrap();
    let c = Instance::new(vec![4, 3], 12346).unwrap();

    let point = [1.3, 2.7];
    assert_eq!(a.noise(&point).unwrap(), b.noise(&point).unwrap());
    assert_ne!(a.noise(&point).unwrap(), c.noise(&point).unwrap());
    assert_eq!(
        a.table(&[3, 3]).unwrap().values(),
        b.table(&[3, 3]).unwrap().values()
    );
}

#[test]
fn noise_rejects_points_outside_the_boundary() {
    let noise = Instance::new(vec![8, 2, 11], 1).unwrap();

    assert!(noise.noise(&[0.0, 0.0, 0.0]).is_ok());
    assert!(noise.noise(&[8.0, 2.0, 11.0]).is_ok());
    assert!(noise.noise(&[4.0, 1.0, 5.5]).is_ok());

    assert!(noise.noise(&[8.5, 1.0, 5.0]).is_err());
    assert!(noise.noise(&[-0.001, 1.0, 5.0]).is_err());
    assert!(noise.noise(&[4.0, 1.0, f64::NAN]).is_err());
    assert!(noise.noise(&[4.0, 1.0]).is_err());
}

#[test]
fn noise_vanishes_on_the_lattice() {
    let noise = Instance::new(vec![3, 2, 2], 99).unwrap();
    for x in 0..=3 {
        for y in 0..=2 {
            for z in 0..=2 {
                let point = [x as f64, y as f64, z as f64];
                let value = noise.noise(&point).unwrap();
                assert!(value.abs() < 1e-12, "noise at {point:?} was {value}");
            }
        }
    }
}

#[test]
fn noise_stays_in_range_and_varies() {
    let noise = Instance::new(vec![5, 5], 2024).unwrap();
    let limit = 2f64.sqrt();

    let mut values = Vec::new();
    for i in 0..=50 {
        for j in 0..=50 {
            let point = [i as f64 / 10.0, j as f64 / 10.0];
            let value = noise.noise(&point).unwrap();
            assert!(value.abs() <= limit, "noise at {point:?} was {value}");
            values.push(value);
        }
    }

    let mean = values.iter().sum::<f64>() / values.len() as f64;
    assert!(mean.abs() < 0.1, "mean was {mean}");
    assert!(
        values.iter().any(|value| value.abs() > 0.05),
        "field is flat"
    );
}

#[test]
fn noise_is_continuous() {
    let noise = Instance::new(vec![4, 4], 5).unwrap();
    // Crossing a cell boundary must not jump, including in the interpolated
    // direction of the neighbouring cell.
    let step = 1e-6;
    for offset in [0.5, 1.25, 2.75] {
        let before = noise.noise(&[2.0 - step, offset]).unwrap();
        let after = noise.noise(&[2.0 + step, offset]).unwrap();
        assert!((before - after).abs() < 1e-4, "{before} vs {after}");
    }
}

#[test]
fn one_dimensional_field_works() {
    let noise = Instance::new(vec![4], 3).unwrap();
    assert_eq!(noise.noise(&[0.0]).unwrap(), 0.0);
    assert!(noise.noise(&[1.5]).unwrap().abs() > 0.0);

    let table = noise.table(&[4]).unwrap();
    assert_eq!(table.shape(), &[17]);
    assert_eq!(table.values().len(), 17);
}

#[test]
fn table_rejects_invalid_divisors() {
    let noise = Instance::new(vec![3, 2], 0).unwrap();
    assert!(noise.table(&[2]).is_err());
    assert!(noise.table(&[2, 0]).is_err());
    assert!(noise.table(&[0, 2]).is_err());
}

#[test]
fn table_refines_each_axis_by_its_divisor() {
    let noise = Instance::new(vec![8, 2, 11], 77).unwrap();
    let table = noise.table(&[2, 1, 4]).unwrap();

    assert_eq!(table.shape(), &[17, 3, 45]);
    assert_eq!(table.values().len(), 17 * 3 * 45);
    assert_eq!(table.get(&[17, 0, 0]), None);
    assert_eq!(table.get(&[0, 0]), None);
}

#[test]
fn table_of_divisor_one_is_the_bare_lattice() {
    let noise = Instance::new(vec![3, 4], 8).unwrap();
    let table = noise.table(&[1, 1]).unwrap();

    assert_eq!(table.shape(), &[4, 5]);
    for value in table.values() {
        assert!(value.abs() < 1e-12, "lattice value was {value}");
    }
}

#[test]
fn table_matches_point_by_point_evaluation() {
    let noise = Instance::new(vec![4, 3, 2], 314).unwrap();
    let divisors = [3, 2, 5];
    let table = noise.table(&divisors).unwrap();
    let shape = table.shape().to_vec();

    for x in 0..shape[0] {
        for y in 0..shape[1] {
            for z in 0..shape[2] {
                let point = [
                    x as f64 / divisors[0] as f64,
                    y as f64 / divisors[1] as f64,
                    z as f64 / divisors[2] as f64,
                ];
                let expected = noise.noise(&point).unwrap();
                let actual = table.get(&[x, y, z]).unwrap();
                assert!(
                    (expected - actual).abs() < 1e-12,
                    "at {point:?}: table {actual} vs point {expected}"
                );
                assert_eq!(actual, table.values()[flat(&shape, &[x, y, z])]);
            }
        }
    }
}

#[test]
fn table_is_row_major_with_the_last_axis_contiguous() {
    let noise = Instance::new(vec![2, 3], 6).unwrap();
    let table = noise.table(&[2, 2]).unwrap();
    let shape = table.shape().to_vec();

    for x in 0..shape[0] {
        for y in 0..shape[1] {
            assert_eq!(
                table.get(&[x, y]).unwrap(),
                table.values()[x * shape[1] + y]
            );
        }
    }
}

#[test]
fn large_table_uses_every_thread_consistently() {
    // Comfortably past the point where the table is split across threads.
    let noise = Instance::new(vec![6, 6, 6], 4242).unwrap();
    let table = noise.table(&[8, 8, 8]).unwrap();
    assert_eq!(table.values().len(), 49 * 49 * 49);

    let single = noise.noise(&[6.0 * 7.0 / 8.0, 3.0, 1.5]).unwrap();
    assert_eq!(table.get(&[42, 24, 12]).unwrap(), single);
    assert!(table.values().iter().all(|value| value.is_finite()));
}

/// Past roughly 4.2M points the shader needs more than one dispatch, so this
/// covers the chunking. Ignored by default because it is slow in debug builds:
/// `cargo test --release --features gpu -- --ignored`.
#[cfg(feature = "gpu")]
#[test]
#[ignore]
fn gpu_table_spanning_several_dispatches_matches_the_cpu_table() {
    let noise = Instance::new(vec![8, 8, 8], 11).unwrap();
    let divisors = [22, 22, 22];

    let gpu = match noise.table_gpu(&divisors) {
        Ok(table) => table,
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };
    let cpu = noise.table(&divisors).unwrap();

    assert!(cpu.values().len() > 4 * 1024 * 1024);
    assert_eq!(cpu.shape(), gpu.shape());
    for (index, (expected, actual)) in cpu.values().iter().zip(gpu.values()).enumerate() {
        assert!(
            (expected - actual).abs() < 1e-5,
            "point {index}: cpu {expected} vs gpu {actual}"
        );
    }
}

#[cfg(feature = "gpu")]
#[test]
fn gpu_table_matches_the_cpu_table() {
    let noise = Instance::new(vec![5, 3, 4], 2718).unwrap();
    let divisors = [4, 6, 3];

    let cpu = noise.table(&divisors).unwrap();
    let gpu = match noise.table_gpu(&divisors) {
        Ok(table) => table,
        // A machine without a usable adapter is not a test failure.
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };

    assert_eq!(cpu.shape(), gpu.shape());
    for (index, (expected, actual)) in cpu.values().iter().zip(gpu.values()).enumerate() {
        assert!(
            (expected - actual).abs() < 1e-5,
            "point {index}: cpu {expected} vs gpu {actual}"
        );
    }
}
