use std::cell::Cell;

use crate::graphics::mesh::Mesh;

use super::math::Vec3;

// WEB-SYS / JS BASED UTILITIES
pub fn get_time() -> f64 {
    return web_sys::window()
        .expect("No global window exists")
        .performance()
        .expect("Window doesn't have Performance")
        .now()
}

thread_local! {
    static SAMPLE_RANDOM: Cell<Option<u64>> = const { Cell::new(None) };
}

fn mix_bits(mut bits: u64) -> u64 {
    bits = (bits ^ (bits >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    bits = (bits ^ (bits >> 27)).wrapping_mul(0x94d049bb133111eb);
    bits ^ (bits >> 31)
}

/// The camera, material and light helpers share this sample's SplitMix64 stream.
pub fn with_sample_random<T>(seed: u32, sample_index: u64, sample: impl FnOnce() -> T) -> T {
    SAMPLE_RANDOM.with(|state| {
        struct Restore<'a>(&'a Cell<Option<u64>>, Option<u64>);
        impl Drop for Restore<'_> {
            fn drop(&mut self) { self.0.set(self.1); }
        }
        let initial = mix_bits(sample_index ^ ((seed as u64) << 32));
        let _restore = Restore(state, state.replace(Some(initial)));
        sample()
    })
}

#[inline]
fn random_bits() -> u64 {
    SAMPLE_RANDOM.with(|state| {
        if let Some(previous) = state.get() {
            let next = previous.wrapping_add(0x9e3779b97f4a7c15);
            state.set(Some(next));
            mix_bits(next)
        } else {
            rand::random()
        }
    })
}

#[inline(always)]
pub fn random_float() -> f32 {
    (random_bits() >> 40) as f32 * (1.0 / 16_777_216.0)
}
#[inline(always)]
pub fn random_range(min: f32, max: f32) -> f32 {
    return random_float() * (max - min) + min;
}
/// Samples uniformly from the range [low, high]
pub fn random_int(min: i32, max: i32) -> i32 {
    assert!(min <= max);
    let count = (i64::from(max) - i64::from(min) + 1) as u64;
    let threshold = count.wrapping_neg() % count;
    loop {
        let bits = random_bits();
        if bits >= threshold {
            return (i64::from(min) + (bits % count) as i64) as i32;
        }
    }
}
/// Samples points in a square in the range [-0.5, 0.5].
#[inline(always)]
pub fn sample_square(side_length: f32) -> (f32, f32) {
    let x = random_float();
    let y = random_float();
    return ((x - 0.5) * side_length, (y - 0.5) * side_length);
}

/// Returns a random x,y coordinate in the unit circle, with even distribution
#[inline(always)]
pub fn sample_circle(radius: f32) -> (f32 ,f32) {
    loop {
        let x = random_range(-1.0, 1.0);
        let y = random_range(-1.0, 1.0);
        if x*x + y*y <= 1.0 {
            return (radius * x, radius * y);
        }
    }
}

// OTHER UTILITIES
pub fn color_to_u8(color: &Vec3) -> (u8, u8, u8) {
    (
        (color.x.clamp(0.0, 1.0) * 255.0) as u8,
        (color.y.clamp(0.0, 1.0) * 255.0) as u8,
        (color.z.clamp(0.0, 1.0) * 255.0) as u8,
    )
}
pub fn color_to_u32(color: &Vec3) -> u32 {
    let (r, g, b) = color_to_u8(color);
    return ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
}
pub fn clamp_color(color: Vec3) -> Vec3 {
    Vec3::new(
        color.x.clamp(0.0, 1.0),
        color.y.clamp(0.0, 1.0),
        color.z.clamp(0.0, 1.0),
    )
}
pub fn shift_color(color: Vec3) -> Vec3 {
    let clamped_color = clamp_color(color);
    let mut avg_color = clamped_color.y;
    avg_color += 0.5;
    if avg_color > 1.0 {
        avg_color -= 1.0;
    }
    return Vec3::new(avg_color, avg_color, avg_color);
}

pub fn gamma_correct_color(color: &Vec3) -> Vec3 {
    Vec3::new(
        color.x.max(0.0).sqrt(),
        color.y.max(0.0).sqrt(),
        color.z.max(0.0).sqrt(),
    )
}

pub fn flip_indices_winding(indices: &mut Vec<usize>) {
    for i in (0..indices.len()).step_by(3) {
        (indices[i], indices[i + 2]) = (indices[i + 2], indices[i]);
    }
}
