use wasm_bindgen::prelude::*;

use crate::graphics::{game::Game, mesh::{Mesh, PhongProperties}, scene_object::SceneObject};
use crate::utils::math::{degrees_to_radians, Vec3};

use super::{bvh::{FlattenedBVH, RayScene}, hittable::Hittable, material::Lambertian};

const CAMERA_POSITION: Vec3 = Vec3 { x: 16.0, y: -18.0, z: 14.0 };
const CAMERA_TARGET: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 0.8 };
const CAMERA_FOV: f32 = 43.0;

fn fixture(full: bool) -> Vec<SceneObject> {
    let (columns, rows) = if full { (249, 200) } else { (19, 16) };
    let vertex = |column: usize, row: usize| {
        let x = 18.0 * column as f32 / columns as f32 - 9.0;
        let y = 14.0 * row as f32 / rows as f32 - 7.0;
        let bank = (y - (x * 0.48).sin()).abs();
        let hill = (bank - 1.0).max(0.0);
        let height = hill * (0.45 + 0.20 * (x * 0.63).cos())
            + 0.08 * hill * (y * 2.4 + x * 1.7).sin();
        Vec3::new(x, y, height)
    };
    let mut vertices = Vec::new();
    let mut colors = Vec::new();
    let mut triangle = |a, b, c, color| {
        vertices.extend([a, b, c]);
        colors.push(color);
    };
    for row in 0..rows {
        for column in 0..columns {
            let a = vertex(column, row);
            let b = vertex(column + 1, row);
            let c = vertex(column + 1, row + 1);
            let d = vertex(column, row + 1);
            let center = (a + b + c + d) * 0.25;
            let color = if center.z < 0.035 {
                Vec3::new(0.045, 0.31, 0.45)
            } else if center.z < 0.27 {
                Vec3::new(0.64, 0.42, 0.21)
            } else if center.z < 1.7 {
                Vec3::new(0.18, 0.40, 0.23)
            } else {
                Vec3::new(0.47, 0.39, 0.32)
            };
            let color = color * (0.86 + 0.14 * (center.x * 2.1 + center.y).cos());
            if column == 0 && row + 1 < rows {
                // A center split adds two triangles while retaining all cell boundaries.
                triangle(a, b, center, color);
                triangle(b, c, center, color);
                triangle(c, d, center, color);
                triangle(d, a, center, color);
            } else {
                triangle(a, b, c, color);
                triangle(a, c, d, color);
            }
        }
    }
    let terrain = Mesh::new_from_non_indexed(vertices, colors, PhongProperties::default());
    let area_light = SceneObject::new_rectangle_light(
        Vec3::new(-5.0, -4.0, 8.0),
        Vec3::new(5.0, 0.0, 0.0),
        Vec3::new(0.0, 4.0, 0.0),
        Vec3::new(5.0, 4.0, 3.0),
        1.0,
        8,
    );
    vec![
        SceneObject::new_from_mesh(terrain, Box::new(Lambertian::default()), false),
        area_light,
    ]
}

fn fixture_hash(game: &Game) -> String {
    // FNV-1a of ordered little-endian f32 geometry, color, material and camera data.
    let mut hash = 0xcbf29ce484222325u64;
    let mut write = |value: f32| {
        for byte in value.to_bits().to_le_bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    };
    for object in game.scene_objects.read().unwrap().iter() {
        for (face, indices) in object.mesh.indices.chunks_exact(3).enumerate() {
            for v in indices.iter().map(|&index| object.mesh.vertices[index])
                .chain([object.mesh.colors[face]])
            {
                for axis in [v.x, v.y, v.z] { write(axis); }
            }
            write(object.get_material_number() as f32);
        }
    }
    for v in [CAMERA_POSITION, CAMERA_TARGET, game.rt_min_sky_color, game.rt_max_sky_color] {
        for axis in [v.x, v.y, v.z] { write(axis); }
    }
    write(CAMERA_FOV);
    write(game.focus_dist);
    write(game.defocus_angle);
    format!("{hash:016x}")
}

#[wasm_bindgen]
pub struct BenchmarkRender {
    game: Game,
    geometry: Option<Vec<Box<dyn Hittable>>>,
    triangles: usize,
    hash: String,
}

#[wasm_bindgen]
impl BenchmarkRender {
    #[wasm_bindgen(constructor)]
    pub fn new(full: bool, width: usize, height: usize, samples: usize, depth: usize, seed: u32) -> Result<Self, String> {
        if !(1..=512).contains(&width) || !(1..=512).contains(&height)
            || !(1..=4096).contains(&samples) || !(1..=64).contains(&depth)
        {
            return Err("Invalid benchmark resolution, sample count or path depth".into());
        }
        let mut game = Game::with_dimensions(width, height);
        game.camera.pos = CAMERA_POSITION;
        game.camera.look_at(&CAMERA_TARGET);
        game.camera.set_fov(degrees_to_radians(CAMERA_FOV));
        game.ray_target_samples = samples;
        game.ray_max_depth = depth;
        game.ray_seed = seed;
        game.rt_min_sky_color = Vec3::new(0.6, 0.63, 0.68);
        game.rt_max_sky_color = Vec3::new(0.28, 0.41, 0.60);
        *game.scene_objects.write().unwrap() = fixture(full);
        game.extract_rt_lights_from_scene_objects();
        let hash = fixture_hash(&game);
        let geometry = game.collect_rt_geometry();
        let triangles = geometry.len();
        let expected = if full { 100_000 } else { 640 };
        if triangles != expected {
            return Err(format!("Expected {expected} converted triangles, found {triangles}"));
        }
        Ok(Self { game, geometry: Some(geometry), triangles, hash })
    }

    pub fn prepare(&mut self, use_bvh: bool) {
        if let Some(geometry) = self.geometry.take() {
            self.game.ray_scene = Some(if use_bvh {
                RayScene::Bvh(FlattenedBVH::new(geometry))
            } else {
                RayScene::BruteForce(geometry)
            });
        }
    }

    pub fn step(&mut self) -> f64 {
        if self.game.ray_scene.is_some() && self.game.ray_samples_accumulated < self.game.ray_target_samples {
            self.game.render_sample_rows(8);
        }
        self.game.ray_samples_accumulated as f64 + self.game.ray_next_row as f64 / self.game.camera.height as f64
    }

    pub fn triangle_count(&self) -> usize { self.triangles }
    pub fn scene_hash(&self) -> String { self.hash.clone() }
    pub fn rgba(&self) -> Vec<u8> { self.game.pixel_buf.get_gamma_corrected_buf_as_u8() }

    pub fn linear_rgb(&self) -> Vec<f32> {
        let mut result = Vec::with_capacity(self.game.camera.width * self.game.camera.height * 3);
        for row in 0..self.game.camera.height {
            for pixel in self.game.pixel_buf.get_row_guard(row).lock().unwrap().iter() {
                result.extend([pixel.x, pixel.y, pixel.z]);
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn fixtures_have_unique_nonzero_faces_after_conversion() {
        for (full, expected) in [(false, 640), (true, 100_000)] {
            let render = BenchmarkRender::new(full, 4, 4, 1, 4, 123).unwrap();
            let objects = render.game.scene_objects.read().unwrap();
            let mut unique = HashSet::new();
            for object in objects.iter() {
                let triangles = object.mesh.to_rt_triangles(object.hittables[0].get_material());
                assert_eq!(triangles.len(), object.hittables.len());
                for triangle in triangles {
                    assert!(triangle.get_area() > 0.0);
                    let mut points = [triangle.origin, triangle.origin + triangle.u, triangle.origin + triangle.v]
                        .map(|v| [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]);
                    points.sort_unstable();
                    assert!(unique.insert(points));
                }
            }
            assert_eq!(unique.len(), expected);
            assert_eq!(render.game.get_rt_lights().len(), 2);
        }
    }

    #[test]
    fn seeded_image_matches_across_algorithms_and_pools() {
        let render = |use_bvh, threads| {
            let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
            pool.install(|| {
                let mut run = BenchmarkRender::new(false, 24, 18, 2, 5, 20261112).unwrap();
                run.prepare(use_bvh);
                while run.step() < 2.0 {}
                run.linear_rgb()
            })
        };
        let baseline = render(false, 1);
        assert!(baseline.iter().any(|&value| value > 0.1));
        assert!(baseline.iter().all(|value| value.is_finite()));
        assert_eq!(baseline, render(true, 1));
        assert_eq!(baseline, render(true, 8));
    }
}
