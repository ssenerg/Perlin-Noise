use perlin_noise::{Globe, Instance, Palette, Relief, Renderer, Shape};

fn renderer(dims: Vec<usize>, seed: u64) -> Renderer {
    Renderer::new(Instance::new(dims, seed).unwrap()).unwrap()
}

#[test]
fn only_wraps_fields_it_can_draw() {
    assert!(Renderer::new(Instance::new(vec![4, 4], 0).unwrap()).is_ok());
    assert!(Renderer::new(Instance::new(vec![4, 4, 2], 0).unwrap()).is_ok());
    assert!(Renderer::new(Instance::new(vec![4, 4, 4, 1], 0).unwrap()).is_ok());
    assert!(Renderer::new(Instance::new(vec![4], 0).unwrap()).is_err());
    assert!(Renderer::new(Instance::new(vec![4, 4, 4, 4, 4], 0).unwrap()).is_err());
}

#[test]
fn each_mode_needs_its_own_dimension_count() {
    let flat = renderer(vec![4, 4], 1);
    assert!(flat.grayscale(&[2, 2]).is_ok());
    assert!(flat.colored(&[2, 2], Palette::Heat).is_ok());
    assert!(flat.relief(&[2, 2], &Relief::default()).is_ok());
    assert!(flat.rgb_channels(&[2, 2]).is_err());
    assert!(flat.rgb_palette(&[2, 2], 0, Palette::Heat).is_err());
    assert!(flat.globe(8, 8, &Globe::default()).is_err());

    let deep = renderer(vec![4, 4, 1], 1);
    assert!(deep.rgb_channels(&[2, 2, 2]).is_ok());
    assert!(deep.rgb_palette(&[2, 2, 2], 0, Palette::Heat).is_ok());
    assert!(deep.grayscale(&[2, 2, 2]).is_err());
    assert!(deep.colored(&[2, 2, 2], Palette::Heat).is_err());
    assert!(deep.relief(&[2, 2, 2], &Relief::default()).is_err());
    assert!(deep.globe(8, 8, &Globe::default()).is_err());

    let round = renderer(vec![3, 3, 3, 1], 1);
    assert!(round.globe(8, 8, &Globe::default()).is_ok());
    assert!(round.grayscale(&[2, 2, 2, 2]).is_err());
    assert!(round.rgb_channels(&[2, 2, 2, 2]).is_err());
    assert!(round.relief(&[2, 2, 2, 2], &Relief::default()).is_err());
}

#[test]
fn image_size_follows_the_refined_lattice() {
    // The first axis runs down the image, the second across it.
    let gray = renderer(vec![8, 2], 5).grayscale(&[4, 16]).unwrap();
    assert_eq!(gray.height(), 8 * 4 + 1);
    assert_eq!(gray.width(), 2 * 16 + 1);
    assert_eq!(gray.channels(), 1);
    assert_eq!(gray.pixels().len(), 33 * 33);

    let color = renderer(vec![8, 2, 1], 5)
        .rgb_channels(&[4, 16, 2])
        .unwrap();
    assert_eq!((color.height(), color.width()), (33, 33));
    assert_eq!(color.channels(), 3);
    assert_eq!(color.pixels().len(), 33 * 33 * 3);
}

#[test]
fn pixel_lookup_matches_the_buffer() {
    let image = renderer(vec![3, 4, 1], 6).rgb_channels(&[3, 3, 2]).unwrap();
    for y in 0..image.height() {
        for x in 0..image.width() {
            let start = (y * image.width() + x) * 3;
            assert_eq!(
                image.pixel(x, y).unwrap(),
                &image.pixels()[start..start + 3]
            );
        }
    }
    assert!(image.pixel(image.width(), 0).is_none());
    assert!(image.pixel(0, image.height()).is_none());
}

#[test]
fn greyscale_spans_the_whole_range() {
    let image = renderer(vec![8, 8], 2024).grayscale(&[16, 16]).unwrap();
    let darkest = *image.pixels().iter().min().unwrap();
    let brightest = *image.pixels().iter().max().unwrap();
    assert_eq!((darkest, brightest), (0, 255));
}

#[test]
fn a_flat_field_renders_mid_grey() {
    // Divisors of 1 sample only the intersections, where noise is exactly 0,
    // so there is no range to stretch and every pixel sits in the middle.
    let image = renderer(vec![4, 4], 3).grayscale(&[1, 1]).unwrap();
    assert!(image.pixels().iter().all(|byte| *byte == 128));
}

#[test]
fn channels_come_from_the_requested_slices() {
    let noise = Instance::new(vec![4, 4, 1], 77).unwrap();
    let divisors = [8, 8, 4];
    let table = noise.table(&divisors).unwrap();
    let depth = table.shape()[2];

    let image = Renderer::new(noise)
        .unwrap()
        .rgb_channels_at(&divisors, [0, 1, depth - 1])
        .unwrap();

    // Ordering within a channel has to survive the mapping to bytes, so the
    // brightest red pixel must be where slice 0 peaks.
    let peak = (0..image.width() * image.height())
        .max_by(|a, b| {
            let value = |index: usize| table.values()[index * depth];
            value(*a).total_cmp(&value(*b))
        })
        .unwrap();
    let reds: Vec<u8> = image.pixels().iter().step_by(3).copied().collect();
    assert_eq!(reds[peak], *reds.iter().max().unwrap());
}

#[test]
fn channels_reject_impossible_slices() {
    let deep = renderer(vec![4, 4, 1], 8);
    // Two samples on the colour axis cannot fill three channels.
    assert!(deep.rgb_channels(&[4, 4, 1]).is_err());
    assert!(deep.rgb_channels_at(&[4, 4, 2], [0, 1, 3]).is_err());
    assert!(deep.rgb_channels_at(&[4, 4, 2], [0, 1, 2]).is_ok());
    assert!(deep.rgb_palette(&[4, 4, 2], 3, Palette::Gray).is_err());
}

#[test]
fn palette_runs_from_end_to_end() {
    for palette in [
        Palette::Gray,
        Palette::BlueRed,
        Palette::Heat,
        Palette::Crimson,
        Palette::Azure,
        Palette::Terrain,
    ] {
        // Out of range positions clamp to the ends.
        assert_eq!(palette.color(-1.0), palette.color(0.0));
        assert_eq!(palette.color(2.0), palette.color(1.0));
        assert_ne!(palette.color(0.0), palette.color(1.0));
    }

    assert_eq!(Palette::Gray.color(0.0), [0, 0, 0]);
    assert_eq!(Palette::Gray.color(1.0), [255, 255, 255]);
    assert_eq!(Palette::Gray.color(0.5), [128, 128, 128]);
}

#[test]
fn palette_image_only_holds_palette_colors() {
    let image = renderer(vec![6, 6, 1], 9)
        .rgb_palette(&[8, 8, 1], 0, Palette::BlueRed)
        .unwrap();

    // Every pixel has to be a blend of two neighbouring stops, which for
    // BlueRed means green never runs ahead of both blue and red at once.
    for pixel in image.pixels().chunks(3) {
        let [red, green, blue] = [pixel[0], pixel[1], pixel[2]];
        assert!(
            green <= red.max(blue) + 1,
            "unexpected colour {red},{green},{blue}"
        );
    }
}

#[test]
fn colored_matches_the_palette_at_the_extremes() {
    let flat = renderer(vec![6, 6], 12);
    let gray = flat.grayscale(&[8, 8]).unwrap();
    let image = flat.colored(&[8, 8], Palette::Terrain).unwrap();

    assert_eq!(image.channels(), 3);
    assert_eq!(
        (image.width(), image.height()),
        (gray.width(), gray.height())
    );

    // Greyscale and colour share one scaling, so the darkest greyscale pixel
    // is the one that gets the bottom of the ramp.
    let darkest = gray.pixels().iter().position(|byte| *byte == 0).unwrap();
    let brightest = gray.pixels().iter().position(|byte| *byte == 255).unwrap();
    let pixel = |index: usize| image.pixels()[index * 3..index * 3 + 3].to_vec();
    assert_eq!(pixel(darkest), Palette::Terrain.color(0.0));
    assert_eq!(pixel(brightest), Palette::Terrain.color(1.0));
}

#[test]
fn relief_shades_a_height_map() {
    let flat = renderer(vec![6, 6], 31);
    let image = flat.relief(&[16, 16], &Relief::default()).unwrap();
    assert_eq!(image.channels(), 3);
    assert_eq!(image.pixels().len(), 97 * 97 * 3);

    // Lighting has to produce both lit and shaded pixels, otherwise the image
    // is flat and there is no relief to see.
    let brightness: Vec<u16> = image
        .pixels()
        .chunks(3)
        .map(|pixel| pixel.iter().map(|byte| u16::from(*byte)).sum())
        .collect();
    let darkest = *brightness.iter().min().unwrap();
    let brightest = *brightness.iter().max().unwrap();
    assert!(brightest > darkest + 200, "{darkest} to {brightest}");
}

#[test]
fn relief_falls_to_the_dark_gradually() {
    let flat = renderer(vec![4, 4], 3);
    let image = flat.relief(&[128, 128], &Relief::default()).unwrap();

    let brightness: Vec<usize> = image
        .pixels()
        .chunks(3)
        .map(|pixel| pixel.iter().map(|byte| usize::from(*byte)).sum::<usize>() / 3)
        .collect();

    // Sorting the pixels into brightness bands shows whether the surface
    // passes through the middle tones or jumps from dark straight to colour.
    // A gap anywhere between the darkest and brightest band would mean the
    // fall to the dark happens in one step.
    let mut bands = [0usize; 16];
    for value in &brightness {
        bands[value * 16 / 256] += 1;
    }
    let darkest = bands.iter().position(|count| *count > 0).unwrap();
    let brightest = bands.iter().rposition(|count| *count > 0).unwrap();
    assert!(brightest - darkest >= 8, "only {} bands used", bands.len());
    assert!(
        bands[darkest..=brightest].iter().all(|count| *count > 0),
        "a brightness band is empty: {bands:?}"
    );

    // And most of the image should be those middle tones rather than either
    // extreme, which is what makes the shading read as a gradient.
    let middle = brightness
        .iter()
        .filter(|value| (30..170).contains(*value))
        .count();
    assert!(middle * 100 / brightness.len() >= 40);
    // The darkening still has to arrive somewhere dark.
    assert!(*brightness.iter().min().unwrap() < 24);
}

#[test]
fn occlusion_darkens_the_low_ground() {
    let flat = renderer(vec![4, 4], 41);
    let total = |occlusion| {
        flat.relief(
            &[16, 16],
            &Relief {
                occlusion,
                ..Default::default()
            },
        )
        .unwrap()
        .pixels()
        .iter()
        .map(|byte| u64::from(*byte))
        .sum::<u64>()
    };

    assert!(total(0.0) > total(0.5));
    assert!(total(0.5) > total(1.0));
}

#[test]
fn relief_shape_changes_the_height_map() {
    let flat = renderer(vec![4, 4], 17);
    let of = |shape| {
        flat.relief(
            &[8, 8],
            &Relief {
                shape,
                ..Default::default()
            },
        )
        .unwrap()
        .into_pixels()
    };

    // Billow and ridged are the same height map upside down, so they light
    // differently; smooth is a third surface again.
    assert_ne!(of(Shape::Billow), of(Shape::Ridged));
    assert_ne!(of(Shape::Billow), of(Shape::Smooth));
}

#[test]
fn moving_the_light_moves_the_shading() {
    let flat = renderer(vec![4, 4], 23);
    let lit_from = |light| {
        flat.relief(
            &[8, 8],
            &Relief {
                light,
                ..Default::default()
            },
        )
        .unwrap()
        .into_pixels()
    };

    let left = lit_from([-0.6, -0.6, 0.5]);
    let right = lit_from([0.6, 0.6, 0.5]);
    assert_ne!(left, right);
    // Opposite lights over the same field should land in the same ballpark;
    // neither direction may blow out or go black.
    let total = |pixels: &[u8]| pixels.iter().map(|byte| u64::from(*byte)).sum::<u64>();
    let difference = total(&left).abs_diff(total(&right));
    assert!(
        difference * 4 < total(&left),
        "{difference} is too lopsided"
    );
}

#[test]
fn a_flat_field_renders_as_an_unlit_surface() {
    // Noise is exactly 0 on the intersections, so there is no slope anywhere
    // and every pixel gets identical shading.
    let image = renderer(vec![4, 4], 3)
        .relief(&[1, 1], &Relief::default())
        .unwrap();
    let first = image.pixel(0, 0).unwrap().to_vec();
    for pixel in image.pixels().chunks(3) {
        assert_eq!(pixel, first.as_slice());
    }
}

#[test]
fn globe_takes_the_size_it_is_asked_for() {
    let round = renderer(vec![3, 3, 3, 1], 88);
    let image = round.globe(120, 80, &Globe::default()).unwrap();
    assert_eq!((image.width(), image.height()), (120, 80));
    assert_eq!(image.channels(), 3);
    assert_eq!(image.pixels().len(), 120 * 80 * 3);

    assert!(round.globe(0, 80, &Globe::default()).is_err());
    assert!(round.globe(80, 0, &Globe::default()).is_err());
    for fill in [0.0, -1.0, 1.5] {
        assert!(
            round
                .globe(
                    80,
                    80,
                    &Globe {
                        fill,
                        ..Default::default()
                    }
                )
                .is_err()
        );
    }
}

#[test]
fn globe_draws_a_circle_of_the_right_size() {
    let globe = Globe {
        fill: 0.8,
        ..Default::default()
    };
    let size = 200;
    let image = renderer(vec![3, 3, 3, 1], 12)
        .globe(size, size, &globe)
        .unwrap();

    let sphere = |x: usize, y: usize| {
        let pixel = image.pixel(x, y).unwrap();
        // A shaded pixel can land on the background colour by chance, so allow
        // a little room around it.
        pixel
            .iter()
            .zip(globe.background)
            .any(|(byte, behind)| byte.abs_diff(behind) > 2)
    };

    // Nothing outside the circle, something at the middle of it.
    for corner in [(0, 0), (size - 1, 0), (0, size - 1), (size - 1, size - 1)] {
        assert!(!sphere(corner.0, corner.1), "{corner:?} is not background");
    }
    assert!(sphere(size / 2, size / 2));

    let radius = globe.fill * size as f64 / 2.0;
    let drawn = (0..size)
        .flat_map(|y| (0..size).map(move |x| (x, y)))
        .filter(|(x, y)| sphere(*x, *y))
        .count();
    let circle = std::f64::consts::PI * radius * radius;
    assert!(
        (drawn as f64 - circle).abs() < circle * 0.05,
        "{drawn} pixels drawn, a circle would be {circle:.0}"
    );
}

#[test]
fn globe_colour_follows_the_fourth_axis_or_the_palette() {
    let round = renderer(vec![3, 3, 3, 2], 19);
    let of = |rgb_from_fourth_axis, palette| {
        round
            .globe(
                96,
                96,
                &Globe {
                    rgb_from_fourth_axis,
                    surface: Relief {
                        palette,
                        ..Globe::default().surface
                    },
                    ..Default::default()
                },
            )
            .unwrap()
            .into_pixels()
    };

    let channels = of(true, Palette::Crimson);
    let ramp = of(false, Palette::Crimson);
    assert_ne!(channels, ramp);
    // The palette is unread while the colour comes off the fourth axis.
    assert_eq!(channels, of(true, Palette::Azure));

    // Crimson is red all the way along, so a sphere coloured by it cannot come
    // out blue; three channels off the field are under no such obligation.
    let bluer = |pixels: &[u8]| {
        pixels
            .chunks(3)
            .filter(|pixel| pixel[2] > pixel[0] + 8)
            .count()
    };
    assert_eq!(bluer(&ramp), 0);
    assert!(bluer(&channels) > 0);
}

#[test]
fn globe_height_lifts_the_surface() {
    let round = renderer(vec![3, 3, 3, 1], 64);
    let of = |height| {
        round
            .globe(
                96,
                96,
                &Globe {
                    surface: Relief {
                        height,
                        ..Globe::default().surface
                    },
                    // Colour off the fourth axis does not depend on the
                    // height, so anything that changes between these renders
                    // changed because the surface tilted.
                    rgb_from_fourth_axis: true,
                    ..Default::default()
                },
            )
            .unwrap()
            .into_pixels()
    };

    // At height 0 the surface is a plain sphere, lit by its curvature alone.
    let plain = of(0.0);
    let shaded_apart = |pixels: &[u8]| {
        pixels
            .chunks(3)
            .zip(plain.chunks(3))
            .filter(|(bumpy, flat)| bumpy.iter().zip(*flat).any(|(a, b)| a.abs_diff(*b) > 8))
            .count()
    };

    // The taller the surface rises, the more of it turns away from the light.
    let some = shaded_apart(&of(0.15));
    let more = shaded_apart(&of(0.5));
    assert!(some > 96 * 96 / 20, "only {some} pixels shaded differently");
    assert!(more > some, "{some} pixels at 0.15, {more} at 0.5");
}

#[test]
fn globe_lighting_answers_to_the_light_and_the_occlusion() {
    let round = renderer(vec![3, 3, 3, 1], 91);
    let lit = |light, occlusion| {
        round
            .globe(
                96,
                96,
                &Globe {
                    surface: Relief {
                        light,
                        occlusion,
                        ..Globe::default().surface
                    },
                    ..Default::default()
                },
            )
            .unwrap()
            .into_pixels()
    };
    let total = |pixels: &[u8]| pixels.iter().map(|byte| u64::from(*byte)).sum::<u64>();

    let left = lit([-0.6, -0.6, 0.5], 0.35);
    let right = lit([0.6, 0.6, 0.5], 0.35);
    assert_ne!(left, right);
    // Opposite lights over the same sphere should land in the same ballpark.
    let difference = total(&left).abs_diff(total(&right));
    assert!(
        difference * 4 < total(&left),
        "{difference} is too lopsided"
    );

    let default = Globe::default().surface.light;
    assert!(total(&lit(default, 0.0)) > total(&lit(default, 0.5)));
    assert!(total(&lit(default, 0.5)) > total(&lit(default, 1.0)));
}

#[test]
fn globe_falls_to_the_dark_gradually() {
    // Colour by height, where a hollow that lights badly also draws the
    // bottom of the ramp, so a hard edge in the shading shows up as one in
    // the picture.
    let image = renderer(vec![7, 7, 7, 2], 3)
        .globe(
            240,
            240,
            &Globe {
                rgb_from_fourth_axis: false,
                ..Default::default()
            },
        )
        .unwrap();

    let mut bands = [0usize; 16];
    let mut washed = 0;
    let mut inside = 0;
    for row in 0..240 {
        for col in 0..240 {
            // Well inside the rim, so neither the background nor the fade
            // into it is counted.
            let x = col as f64 + 0.5 - 120.0;
            let y = row as f64 + 0.5 - 120.0;
            if (x * x + y * y).sqrt() > 0.85 * 0.92 * 120.0 {
                continue;
            }
            inside += 1;

            let pixel = image.pixel(col, row).unwrap();
            let value = pixel.iter().map(|byte| usize::from(*byte)).sum::<usize>() / 3;
            bands[value * 16 / 256] += 1;
            // A bright pixel with hardly any red lead over the other channels
            // is a white highlight smeared over a dark hollow, which reads as
            // a grey cast on what should be a red sphere.
            if pixel[0] > 60
                && usize::from(pixel[1].min(pixel[2])) * 100 > usize::from(pixel[0]) * 55
            {
                washed += 1;
            }
        }
    }

    assert_eq!(washed, 0, "{washed} of {inside} pixels are washed grey");
    // The surface has to pass through the middle tones on its way down. A gap
    // anywhere between the bands in use would mean it fell in one step.
    let darkest = bands.iter().position(|count| *count > 0).unwrap();
    let brightest = bands.iter().rposition(|count| *count > 0).unwrap();
    assert!(brightest - darkest >= 4, "only {} bands used", bands.len());
    assert!(
        bands[darkest..=brightest].iter().all(|count| *count > 0),
        "a brightness band is empty: {bands:?}"
    );
    // And a good part of it has to arrive somewhere near black, rather than
    // stopping at a grey.
    assert!(
        bands[0] * 100 / inside >= 5,
        "only {}% of the sphere is near black",
        bands[0] * 100 / inside
    );
}

#[test]
fn globe_samples_each_pixel_as_often_as_asked() {
    let round = renderer(vec![5, 5, 5, 2], 4);
    let of = |samples| {
        round
            .globe(
                80,
                80,
                &Globe {
                    samples,
                    ..Default::default()
                },
            )
            .map(|image| image.into_pixels())
    };

    // More samples per pixel is a finer average of the same surface, so the
    // picture moves but not by much.
    let coarse = of(1).unwrap();
    let fine = of(4).unwrap();
    assert_ne!(coarse, fine);
    let gap: u64 = coarse
        .iter()
        .zip(&fine)
        .map(|(one, other)| u64::from(one.abs_diff(*other)))
        .sum();
    assert!(gap / coarse.len() as u64 <= 2, "{gap} apart in total");

    assert!(of(0).is_err());
    assert!(of(perlin_noise::MAX_SAMPLES).is_ok());
    assert!(of(perlin_noise::MAX_SAMPLES + 1).is_err());
}

#[test]
fn globe_is_the_same_picture_every_time() {
    let of = || {
        renderer(vec![4, 4, 4, 1], 7)
            .globe(80, 80, &Globe::default())
            .unwrap()
            .into_pixels()
    };
    assert_eq!(of(), of());
}

#[cfg(feature = "png")]
#[test]
fn writes_png_files() {
    let dir = std::env::temp_dir().join("perlin-noise-tests");
    std::fs::create_dir_all(&dir).unwrap();

    for (name, image) in [
        (
            "gray.png",
            renderer(vec![4, 4], 1).grayscale(&[8, 8]).unwrap(),
        ),
        (
            "color.png",
            renderer(vec![4, 4, 1], 1).rgb_channels(&[8, 8, 2]).unwrap(),
        ),
    ] {
        let path = dir.join(name);
        image.write_png(&path).unwrap();

        let written = std::fs::read(&path).unwrap();
        assert_eq!(&written[1..4], b"PNG");
        // Width and height live in the IHDR chunk, big endian at byte 16.
        let dimensions = |offset: usize| {
            u32::from_be_bytes(written[offset..offset + 4].try_into().unwrap()) as usize
        };
        assert_eq!(dimensions(16), image.width());
        assert_eq!(dimensions(20), image.height());
    }
}

#[cfg(feature = "gpu")]
#[test]
fn gpu_renders_the_same_image() {
    let expected = renderer(vec![5, 5], 4242).grayscale(&[9, 9]).unwrap();
    let actual = match renderer(vec![5, 5], 4242).with_gpu(true).grayscale(&[9, 9]) {
        Ok(image) => image,
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };

    assert_eq!(actual.width(), expected.width());
    // The shader's 32 bit floats can round a value onto the next byte.
    for (index, (want, got)) in expected.pixels().iter().zip(actual.pixels()).enumerate() {
        assert!(
            want.abs_diff(*got) <= 1,
            "pixel {index}: cpu {want} vs gpu {got}"
        );
    }
}
