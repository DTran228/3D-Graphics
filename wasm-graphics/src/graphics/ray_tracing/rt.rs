use std::{f32::consts::PI, fmt::Debug, sync::RwLock};

use rayon::prelude::*;

use crate::{graphics::scene_object::SceneObject, utils::{math::{degrees_to_radians, Vec3}, utils::{random_float, random_range, sample_circle, sample_square, with_sample_random}}};

use super::{super::{game::Game, mesh::{Mesh, PhongProperties}}, bvh::RayScene, material::{Dielectric, Lambertian, Material, Metal}};

// const SAMPLES: usize = 10; // 10
// const MAX_DEPTH: usize = 10; // 10

#[derive(Debug, Clone, Default)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
}

impl Ray {
    pub fn new(origin: Vec3, direction: Vec3) -> Self {
        Self { origin, direction }
    }
    pub fn at(&self, t: f32) -> Vec3 {
        self.origin + self.direction * t
    }
}

#[derive(Debug, Default)]
pub struct HitRecord<'a> {
    pub t: f32,
    pub pos: Vec3,
    pub normal: Vec3,
    pub front_face: bool,
    pub surface_color: Vec3,
    pub material: Option<&'a dyn Material>,
}

impl HitRecord<'_> {
    pub fn set_face_normal(&mut self, ray: &Ray, outward_normal: Vec3) {
        self.front_face = ray.direction.dot(outward_normal) < 0.0;
        self.normal = if self.front_face { outward_normal } else { -outward_normal };
    }
}

impl Game {

    /// Render a bounded set of rows for one sample, then advance the cursor.
    pub fn render_sample_rows(&mut self, rows: usize) {
        let first = self.ray_next_row;
        let end = (first + rows).min(self.camera.height);
        let samples = self.ray_samples_accumulated;
        let scene = self.ray_scene.as_ref().expect("Prepare ray geometry before rendering");
        let width = self.camera.width;
        let sample_offset = samples as u64 * width as u64 * self.camera.height as u64;

        (first..end).into_par_iter().for_each(|y| {
            let mut row = self.pixel_buf.get_row_guard(y).lock().unwrap();
            for (x, pixel) in row.iter_mut().enumerate() {
                let sample_index = sample_offset + (y * width + x) as u64;
                let color = with_sample_random(self.ray_seed, sample_index, || {
                    let ray = self.get_rand_ray_at_pixel_with_defocus(x, y);
                    self.ray_trace_mis(scene, ray, self.ray_max_depth)
                });
                *pixel = (*pixel * samples as f32 + color) / (samples + 1) as f32;
            }
        });

        self.ray_next_row = end;
        if end == self.camera.height {
            self.ray_next_row = 0;
            self.ray_samples_accumulated += 1;
        }
    }

    fn ray_trace_mis(&self, scene: &RayScene, mut ray: Ray, mut depth: usize) -> Vec3 {

        let use_direct = true;

        // throughput will hold the multiplied indirect attenuations, start with identity for multiplication = 1
        // accum color will hold the accumulated direct lighting, start with 0
        let mut throughput = Vec3::new(1.0, 1.0, 1.0);
        let mut accum_color = Vec3::zero();

        while depth > 0 {
            depth -= 1;
            let mut hit_record = HitRecord::default();
            let mut hit_anything = false;

            // russian-roulette optimization
            if depth >= self.ray_max_depth.saturating_sub(3) {
                // skip russian roulette on first bounces
            } else {
                // use luminance
                let lum = 0.2126*throughput.x + 0.7152*throughput.y + 0.0722*throughput.z;
                let p_continue = lum.clamp(0.05, 1.0);
                if random_float() > p_continue {
                    break;
                }
                throughput /= p_continue;
            }

            if scene.hit(&ray, 0.001, 5000.0, &mut hit_record) {
                hit_anything = true;
            }

            if hit_anything {
                let successful_scatter;
                let attenuation;
                let scattered_ray;
                let light_sampling_option;

                if let Some(material) = hit_record.material {

                    let emitted_color = material.emitted(&hit_record);
                    (successful_scatter, attenuation, scattered_ray, light_sampling_option) = material.scatter_mis(&ray, &hit_record, self.get_rt_lights());
                    if successful_scatter {

                        let mut direct_lighting = Vec3::zero();
                        if use_direct {
                            if let Some((direct_attenuation, shadow_ray, light_dist)) = light_sampling_option {
                                let mut shadow_ray_hit_record = HitRecord::default();
                                let light_is_occluded = scene.hit(&shadow_ray, 0.001, light_dist - 0.001, &mut shadow_ray_hit_record);
                                if !light_is_occluded {
                                    direct_lighting = direct_attenuation;
                                }
                            }
                        }

                        accum_color += throughput.mul_elementwise(emitted_color);
                        accum_color += throughput.mul_elementwise(direct_lighting);
                        throughput.mul_elementwise_inplace(attenuation);

                        ray = scattered_ray;
                    } else {
                        return accum_color + throughput.mul_elementwise(emitted_color);
                        // no scatter, terminate here with current emitted color
                    }

                } else {
                    return Vec3::zero(); // no material, return black
                }

            } else { // if hit_anything == false, then ray hit nothing, goes off into sky
                let sky_color = self.get_rt_sky_color(&ray.direction.normalized());
                accum_color += throughput.mul_elementwise(sky_color);
                return accum_color;
            }
        }

        // max depth reached, return accumulated color
        return accum_color;
    }

    fn get_ray_at_pixel(&self, x: usize, y: usize) -> Ray {
        let origin = self.camera.pos;

        let mut v = Vec3::new(x as f32, y as f32, 1.0);
        self.camera.vertex_screen_to_world_space(&mut v);
        let direction = (v - origin).normalized();

        Ray::new(origin, direction)
    }

    fn get_rand_ray_at_pixel(&self, x: usize, y: usize) -> Ray {
        let origin = self.camera.pos;
        let (offset_x, offset_y) = sample_square(1.0);
        let mut v = Vec3::new(x as f32 + offset_x, y as f32 + offset_y, 1.0);
        self.camera.vertex_screen_to_world_space(&mut v);
        let direction = (v - origin).normalized();

        Ray::new(origin, direction)
    }

    fn get_rand_ray_at_pixel_with_defocus(&self, x: usize, y: usize) -> Ray {
        let defocus_disk_radius = (0.5 * self.defocus_angle).tan() * self.focus_dist;
        let (disk_x, disk_y) = sample_circle(defocus_disk_radius);
        let (offset_x, offset_y) = sample_square(1.0);


        let mut point_on_focus_plane = Vec3::new(x as f32 + offset_x, y as f32 + offset_y, self.focus_dist);
        self.camera.vertex_screen_to_camera_space(&mut point_on_focus_plane);

        let mut point_on_defocus_disk = Vec3::new(0.0, disk_x, disk_y);

        self.camera.vertex_camera_to_world_space(&mut point_on_focus_plane);
        self.camera.vertex_camera_to_world_space(&mut point_on_defocus_disk);

        let ray_dir = (point_on_focus_plane - point_on_defocus_disk).normalized();

        return Ray::new(point_on_defocus_disk, ray_dir);
    }

    // pub fn create_bvh_from_scene_objs(&mut self) {
    //     let hittables = self.scene_objects
    //         .borrow()
    //         .iter()
    //         .flat_map(|o| o.hittables.iter().map(|h| h.clone_box()))
    //         .collect();
    //     self.bvh = Some(BVHNode::new(hittables));
    // }

    pub fn create_rt_test_scene_spheres(&mut self) {

        self.pre_scene_load();

        self.max_sky_color = Vec3::new(0.5, 0.7, 1.0);
        self.min_sky_color = Vec3::new(1.0, 1.0, 1.0);

        self.rt_max_sky_color = Vec3::new(0.5, 0.7, 1.0);
        self.rt_min_sky_color = Vec3::new(1.0, 1.0, 1.0);

        self.ray_max_depth = 20;

        let ground_material = Lambertian::default();
        let sphere = SceneObject::new_sphere(
            Vec3::new(0.0, 0.0, -1000.0),
            1000.0,
            Vec3::new(0.5, 0.5, 0.5),
            4,
            SceneObject::new_diffuse_mat(),
        );
        self.add_scene_object(sphere);

        let lim = 11; // 11

        for a in -lim..lim {
            for b in -lim..lim {
                let choose_mat = random_float();
                let center = Vec3::new(a as f32 + 0.9*random_float(), b as f32 + 0.9*random_float(), 0.2);

                if (center - Vec3::new(4.0, 0.0, 0.2)).len() > 0.9 {
                    let sphere_material: Box<dyn Material>;
                    let color;

                    let unified_mat;

                    if choose_mat < 0.8 {
                        // diffuse
                        color = Vec3::random().mul_elementwise(Vec3::random());
                        sphere_material = Lambertian::default().clone_box();
                        unified_mat = SceneObject::new_diffuse_mat();
                    } else if choose_mat < 0.95 {
                        // metal
                        color = Vec3::random() * 0.5 + Vec3::new(0.5, 0.5, 0.5);
                        let fuzz = random_range(0.0, 0.5);
                        sphere_material = Metal::new(fuzz).clone_box();
                        unified_mat = SceneObject::new_metal_mat(fuzz)
                    } else {
                        // glass
                        color = Vec3::new(1.0, 1.0, 1.0);
                        sphere_material = Dielectric::new(1.5).clone_box();
                        unified_mat = SceneObject::new_glass_mat(0.5, 1.5);
                    }
                    
                    // let sphere = Sphere::new(center, 0.2, color, 2, sphere_material);
                    // self.add_mesh(sphere.to_mesh(2, PhongProperties::default()));
                    
                    let sphere = SceneObject::new_sphere(center, 0.2, color, 2, unified_mat);
                    self.add_scene_object(sphere);
                }
            }
        }

        // glass
        let sphere = SceneObject::new_sphere(
            Vec3::new(0.0, 0.0, 1.0), 
            1.0, 
            Vec3::new(1.0, 1.0, 1.0), 
            4,
            SceneObject::new_glass_mat(0.5, 1.5)
        );
        self.add_scene_object(sphere);

        // lambertian
        let sphere = SceneObject::new_sphere(
            Vec3::new(-4.0, 0.0, 1.0), 
            1.0 ,
            Vec3::new(0.4, 0.2, 0.1), 
            4,
            SceneObject::new_diffuse_mat(),
        );
        self.add_scene_object(sphere);

        // metal
        let sphere = SceneObject::new_sphere(
            Vec3::new(4.0, 0.0, 1.0), 
            1.0, 
            Vec3::new(0.7, 0.6, 0.5), 
            4,
            SceneObject::new_metal_mat(0.0)
        );
        self.add_scene_object(sphere);

        self.camera.set_fov(degrees_to_radians(20.0));
        self.camera.pos = Vec3::new(13.0, 3.0, 2.0);
        self.camera.look_at(&Vec3::zero());

        self.defocus_angle = degrees_to_radians(0.6);
        self.focus_dist = 10.0;

        self.post_scene_load();
    
    }

    pub fn create_rt_test_scene_quads(&mut self) {

        self.pre_scene_load();

        self.ray_max_depth = 10;

        let origins = vec![
            Vec3::new(-3.0, 5.0, -2.0),
            Vec3::new(-2.0, 0.0, -2.0),
            Vec3::new(3.0, 1.0, -2.0),
            Vec3::new(-2.0, 1.0, 3.0),
            Vec3::new(-2.0, 5.0, -3.0),
        ];
        let u_vectors = vec![
            Vec3::new(0.0, -4.0, 0.0),
            Vec3::new(4.0, 0.0, 0.0),
            Vec3::new(0.0, 4.0, 0.0),
            Vec3::new(4.0, 0.0, 0.0),
            Vec3::new(4.0, 0.0, 0.0),
        ];
        let v_vectors = vec![
            Vec3::new(0.0, 0.0, 4.0),
            Vec3::new(0.0, 0.0, 4.0),
            Vec3::new(0.0, 0.0, 4.0),
            Vec3::new(0.0, 4.0, 0.0),
            Vec3::new(0.0, -4.0, 0.0),
        ];
        let colors = vec![
            Vec3::new(1.0, 0.2, 0.2),
            Vec3::new(0.2, 1.0, 0.2),
            Vec3::new(0.2, 0.2, 1.0),
            Vec3::new(1.0, 0.5, 0.0),
            Vec3::new(0.2, 0.8, 0.8),
        ];

        let mut mesh_vertices = Vec::new();
        let mut mesh_colors = Vec::new();

        for i in 0..origins.len() {
            let origin = origins[i];
            let u = u_vectors[i];
            let v = v_vectors[i];
            let color = colors[i];

            mesh_vertices.push(origin);
            mesh_vertices.push(origin+u);
            mesh_vertices.push(origin+v);

            mesh_vertices.push(origin+u);
            mesh_vertices.push(origin+u+v);
            mesh_vertices.push(origin+v);

            mesh_colors.push(color);
            mesh_colors.push(color);

            // let t1 = Triangle::new_from_directions(origin, u, v, Vec3::new(1.0, 0.2, 0.2), &lambertian);
            // let t2 = Triangle::new_from_directions(origin+u+v, -u, -v, Vec3::new(1.0, 0.2, 0.2), &lambertian);
        }

        let mesh = Mesh::new_from_non_indexed(mesh_vertices, mesh_colors, PhongProperties::default());
        let scene_obj = SceneObject::new_from_mesh(mesh, Lambertian::default().clone_box(), true);
        self.add_scene_object(scene_obj);

        self.camera.set_fov(degrees_to_radians(80.0));
        self.camera.pos = Vec3::new(0.0, 9.0, 0.0);
        self.camera.look_at(&Vec3::zero());

        self.post_scene_load();
    }

    pub fn create_rt_test_scene_simple_light(&mut self) {

        self.pre_scene_load();

        self.ray_max_depth = 50;

        let sphere = SceneObject::new_sphere(
            Vec3::new(0.0, 0.0, -1000.0), 
            1000.0, 
            Vec3::new(0.9, 0.9, 0.9), 
            4,
            SceneObject::new_diffuse_mat()
        );
        self.add_scene_object(sphere);

        let sphere = SceneObject::new_sphere(
            Vec3::new(0.0, 0.0, 2.0), 
            2.0, 
            Vec3::new(0.9, 0.9, 0.9), 
            4,
            SceneObject::new_diffuse_mat()
        );
        self.add_scene_object(sphere);

        // let (t1, t2) = Triangle::new_quad(
        //     Vec3::new(3.0, -2.0, 1.0),
        //     Vec3::new(0.0, 0.0, 2.0),
        //     Vec3::new(2.0, 0.0, 0.0),
        //     Vec3::new(4.0, 4.0, 4.0), 
        //     &DiffuseLight::default()
        // );
        // self.add_mesh(t1.to_mesh(PhongProperties::default()));
        // self.add_mesh(t2.to_mesh(PhongProperties::default()));
        // rt_objects.push(Box::new(t1.clone()));
        // rt_objects.push(Box::new(t2.clone()));
        // self.rt_lights.push(Box::new(t1));
        // self.rt_lights.push(Box::new(t2));
        let light_rec = SceneObject::new_rectangle_light(
            Vec3::new(3.0, -2.0, 1.0),
            Vec3::new(0.0, 0.0, 2.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(4.0, 4.0, 4.0),
            0.1, 1000);
        self.add_scene_object(light_rec);

        self.max_sky_color = Vec3::new(0.05, 0.05, 0.05);
        self.min_sky_color = Vec3::zero();

        self.rt_max_sky_color = Vec3::zero();
        self.rt_min_sky_color = Vec3::zero();

        self.camera.pos = Vec3::new(26.0, 6.0, 3.0);
        self.camera.look_at(&Vec3::new(0.0, 0.0, 2.0));

        self.camera.set_fov(degrees_to_radians(20.0));
        self.defocus_angle = 0.0;

        self.post_scene_load();
    }

    pub fn create_rt_test_scene_cornell(&mut self) {
        self.pre_scene_load();

        self.ray_max_depth = 20;

        self.max_sky_color = Vec3::new(0.1, 0.1, 0.1);
        self.min_sky_color = Vec3::zero();

        self.rt_max_sky_color = Vec3::zero();
        self.rt_min_sky_color = Vec3::zero();
    
        let red_color = Vec3::new(0.65, 0.05, 0.05);
        let green_color = Vec3::new(0.12, 0.45, 0.15);
        let white_color = Vec3::new(0.73, 0.73, 0.73);
        let light_color = Vec3::new(15.0, 15.0, 15.0);
    
        let lambert_mat = SceneObject::new_diffuse_mat();
    
        self.scene_objects = RwLock::new(vec![
            SceneObject::new_rectangle(
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.0, 55.5, 0.0),
                Vec3::new(0.0, 0.0, 55.5),
                green_color,
                lambert_mat.clone(),
                true,
            ),
            SceneObject::new_rectangle(
                Vec3::new(55.5, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 55.5),
                Vec3::new(0.0, 55.5, 0.0),
                red_color,
                lambert_mat.clone(),
                true,
            ),
            SceneObject::new_rectangle_light(
                Vec3::new(36.3, 35.2, 55.4),
                Vec3::new(0.0, -14.5, 0.0),
                Vec3::new(-17.0, 0.0, 0.0),
                light_color,
                0.1,
                1000,
            ),
            SceneObject::new_rectangle(
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(55.5, 0.0, 0.0),
                Vec3::new(0.0, 55.5, 0.0),
                white_color,
                lambert_mat.clone(),
                true,
            ),
            SceneObject::new_rectangle(
                Vec3::new(55.5, 55.5, 55.5),
                Vec3::new(0.0, -55.5, 0.0),
                Vec3::new(-55.5, 0.0, 0.0),
                white_color,
                lambert_mat.clone(),
                true,
            ),
            SceneObject::new_rectangle(
                Vec3::new(0.0, 55.5, 0.0),
                Vec3::new(55.5, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 55.5),
                white_color,
                lambert_mat.clone(),
                true,
            ),
        ]);
    
        // Left box
        let mut left_box = SceneObject::new_box_from_corners(
            Vec3::new(27.1, 29.5, 0.0),
            Vec3::new(10.6, 46.0, 33.0),
            white_color,
            lambert_mat.clone(),
        );
        left_box.rotate_around_center(degrees_to_radians(15.0), 0.0);
        self.add_scene_object(left_box);
    
        // Right box
        let mut right_box = SceneObject::new_box_from_corners(
            Vec3::new(45.1, 6.5, 0.0),
            Vec3::new(28.6, 23.0, 16.5),
            white_color,
            lambert_mat.clone(),
        );
        right_box.rotate_around_center(degrees_to_radians(-18.0), 0.0);
        self.add_scene_object(right_box);
    
        // Camera
        self.camera.set_fov(degrees_to_radians(40.0));
        self.camera.pos = Vec3::new(27.8, -80.0, 27.8);
        self.camera.look_at(&Vec3::new(27.8, 0.0, 27.8));
        self.defocus_angle = 0.0;
    
        // Updating UI and rebuilding BVH
        self.post_scene_load();
    }


    pub fn create_rt_test_scene_cornell_plus_plus(&mut self, stl_bytes: &[u8]) {
        self.pre_scene_load();

        self.ray_max_depth = 50;
        
        self.max_sky_color = Vec3::new(0.1, 0.1, 0.1);
        self.min_sky_color = Vec3::zero();

        self.rt_max_sky_color = Vec3::zero();
        self.rt_min_sky_color = Vec3::zero();
    
        // let red_color = Vec3::new(0.65, 0.05, 0.05);
        // let green_color = Vec3::new(0.12, 0.45, 0.15);
        let red_color = Vec3::new(1.0, 0.2, 0.2);
        let green_color = Vec3::new(0.2, 1.0, 0.2);
        let grey_color = Vec3::new(0.73, 0.73, 0.73);
        let light_color = Vec3::new(10.0,  10.0, 10.0);
    
        let lambert_mat = SceneObject::new_diffuse_mat();
        let metal_mat = SceneObject::new_metal_mat(0.0);
    
        self.scene_objects = RwLock::new(vec![
            SceneObject::new_rectangle(
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.0, 55.5, 0.0),
                Vec3::new(0.0, 0.0, 55.5),
                green_color,
                metal_mat.clone(),
                true,
            ),
            SceneObject::new_rectangle(
                Vec3::new(55.5, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 55.5),
                Vec3::new(0.0, 55.5, 0.0),
                red_color,
                metal_mat.clone(),
                true,
            ),
            SceneObject::new_rectangle_light(
                Vec3::new(36.3, 35.2, 55.4),
                Vec3::new(0.0, -14.5, 0.0),
                Vec3::new(-17.0, 0.0, 0.0),
                light_color,
                0.1,
                1000,
            ),
            SceneObject::new_rectangle(
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(55.5, 0.0, 0.0),
                Vec3::new(0.0, 55.5, 0.0),
                grey_color,
                lambert_mat.clone(),
                true,
            ),
            SceneObject::new_rectangle(
                Vec3::new(55.5, 55.5, 55.5),
                Vec3::new(0.0, -55.5, 0.0),
                Vec3::new(-55.5, 0.0, 0.0),
                grey_color,
                lambert_mat.clone(),
                true,
            ),
            SceneObject::new_rectangle(
                Vec3::new(0.0, 55.5, 0.0),
                Vec3::new(55.5, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 55.5),
                grey_color,
                lambert_mat.clone(),
                true,
            ),
        ]);
    
        // Left statue
        let statue_color = Vec3::new(0.8, 0.8, 0.8);
        let (statue_phong, statue_mat) = SceneObject::new_diffuse_mat();
        let mut statue_mesh = Mesh::new_from_stl_bytes(stl_bytes, statue_color, statue_phong);
        statue_mesh.scale_by(0.15);
        statue_mesh.rotate_around_center(-PI/2.0, PI/2.0);
        statue_mesh.set_center(Vec3::new(18.5, 37.0, 24.75));
        statue_mesh.rotate_around_center(-degrees_to_radians(15.0), 0.0);
        let statue_obj = SceneObject::new_from_mesh(
            statue_mesh, 
            statue_mat.clone_box(),
            true
        );
        self.add_scene_object(statue_obj);
    
        // Right sphere
        let glass_mat = SceneObject::new_glass_mat(0.5, 1.5);
        let right_sphere = SceneObject::new_sphere(
            Vec3::new(36.8, 15.0, 12.0), 
            12.0,
            Vec3::white(),
            4, 
            glass_mat.clone()
        );
        self.add_scene_object(right_sphere);
    
        // Camera
        self.camera.set_fov(degrees_to_radians(40.0));
        self.camera.pos = Vec3::new(27.8, -80.0, 27.8);
        self.camera.look_at(&Vec3::new(27.8, 0.0, 27.8));
        self.defocus_angle = 0.0;

        // Updating UI and rebuilding BVH
        self.post_scene_load();
    }
}
