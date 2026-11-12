use std::{cell::RefCell, usize::MAX};

use web_sys::console;

use crate::{console_log, graphics::mesh::Mesh, utils::{math::Vec3, utils::random_int}};

use super::{hittable::{self, Hittable}, material::Material, rt::{HitRecord, Ray}};

/// Axis-Aligned Bounding Box (AABB)
/// A bounding box defined by two points: the minimum and maximum corners.
#[derive(Clone, Debug)]
pub struct AABoundingBox {
    pub min: Vec3,
    pub max: Vec3,
}

impl AABoundingBox {
    /// Assumes input vectors are already min and max
    pub fn new_from_sorted(min: Vec3, max: Vec3) -> Self {
        AABoundingBox { min, max }
    }
    pub fn new_from_unsorted(v1: Vec3, v2: Vec3) -> Self {
        let (min, max) = (v1.min_elementwise(v2), v1.max_elementwise(v2));
        return Self::new_from_sorted(min, max);
    }
    pub fn from_sub_boxes(box1: &AABoundingBox, box2: &AABoundingBox) -> Self {
        return AABoundingBox::new_from_sorted(
            box1.min.min_elementwise(box2.min),
            box1.max.max_elementwise(box2.max),
        )
    }
    pub fn universe() -> Self {
        AABoundingBox { 
            min: Vec3::new(-99999.0, -99999.0, -99999.9), 
            max: Vec3::new(99999.0, 99999.0, 99999.0),
        }
    }
    pub fn empty() -> Self {
        AABoundingBox { 
            min: Vec3::new(99999.0, 99999.0, 99999.0),
            max: Vec3::new(-99999.0, -99999.0, -99999.9), 
        }
    }

    pub fn get_x_bounds(&self) -> (f32, f32) {
        return (self.min.x, self.max.x);
    }
    pub fn get_y_bounds(&self) -> (f32, f32) {
        return (self.min.y, self.max.y);
    }
    pub fn get_z_bounds(&self) -> (f32, f32) {
        return (self.min.z, self.max.z);
    }
    pub fn get_x_size(&self) -> f32 {
        return self.max.x - self.min.x;
    }
    pub fn get_y_size(&self) -> f32 {
        return self.max.y - self.min.y;
    }
    pub fn get_z_size(&self) -> f32 {
        return self.max.z - self.min.z;
    }
    pub fn get_center(&self) -> Vec3 {
        return 0.5 * (self.min + self.max);
    }

    pub fn get_longest_axis(&self) -> i32 {
        if self.max.x - self.min.x > self.max.y - self.min.y {
            if self.max.x - self.min.x > self.max.z - self.min.z {
                return 0;
            } else {
                return 2;
            }
        } else {
            if self.max.y - self.min.y > self.max.z - self.min.z {
                return 1;
            } else {
                return 2;
            }
        }
    }

    pub fn pad_to_minimums(&mut self) {
        let delta = 0.0001;
        if self.get_x_size() < delta {
            self.min.x -= 0.5 * delta;
            self.max.x += 0.5 * delta;
        }
        if self.get_y_size() < delta {
            self.min.y -= 0.5 * delta;
            self.max.y += 0.5 * delta;
        }
        if self.get_z_size() < delta {
            self.min.z -= 0.5 * delta;
            self.max.z += 0.5 * delta;
        }
    }

    #[inline]
    fn hit(&self, ray: &Ray, mut t_min: f32, mut t_max: f32) -> bool {
        let axes = [
            (ray.origin.x, ray.direction.x, self.min.x, self.max.x),
            (ray.origin.y, ray.direction.y, self.min.y, self.max.y),
            (ray.origin.z, ray.direction.z, self.min.z, self.max.z),
        ];
        for (origin, direction, min, max) in axes {
            if direction == 0.0 {
                if origin < min || origin > max {
                    return false;
                }
                continue;
            }
            let near = (min - origin) / direction;
            let far = (max - origin) / direction;
            t_min = t_min.max(near.min(far));
            t_max = t_max.min(near.max(far));
            if t_min > t_max {
                return false;
            }
        }
        true
    }
}


#[derive(Debug)]
pub enum BVHNode {
    Leaf {
        bounding_box: AABoundingBox,
        object: Box<dyn Hittable>,
    },
    Internal {
        bounding_box: AABoundingBox,
        left: Box<BVHNode>,
        right: Box<BVHNode>,
    }
}

impl BVHNode {
    pub fn new(mut objects: Vec<Box<dyn Hittable>>) -> Self {

        // leaf node
        if objects.len() == 1 {
            return Self::Leaf { 
                bounding_box: objects[0].get_bounding_box().clone(), 
                object: objects.pop().unwrap() 
            };
        } 
    
        // internal node
        let mut bounding_box = AABoundingBox::empty();
        for obj in objects.iter() {
            bounding_box = AABoundingBox::from_sub_boxes(&bounding_box, obj.get_bounding_box());
        }

        let axis = bounding_box.get_longest_axis();
        objects.sort_by(|obj1, obj2| {
            let c1 = obj1.get_bounding_box().get_center();
            let c2 = obj2.get_bounding_box().get_center();
            match axis {
                0 => (c1.x).partial_cmp(&c2.x).unwrap(),
                1 => (c1.y).partial_cmp(&c2.y).unwrap(),
                2 => (c1.z).partial_cmp(&c2.z).unwrap(),
                _ => unreachable!("invalid axis to sort by"),
            }
        });
        
        let mid = objects.len() / 2;
        let right_vec = objects.split_off(mid);
        let left_vec = objects;

        let left = Box::new(BVHNode::new(left_vec));
        let right = Box::new(BVHNode::new(right_vec));

        let bounding_box = AABoundingBox::from_sub_boxes(
            &left.get_bounding_box(), 
            &right.get_bounding_box()
        );

        return BVHNode::Internal { 
            bounding_box, 
            left,
            right,
        };

    }

    #[inline(always)]
    pub fn hit<'a>(&'a self, ray: &Ray, t_min: f32, t_max: f32, hit_record: &mut HitRecord<'a>) -> bool {
        match self {
            BVHNode::Leaf { object, .. } => {
                return object.hit(ray, t_min, t_max, hit_record)
            },
            BVHNode::Internal { bounding_box, left, right } => {
                if !bounding_box.hit(ray, t_min, t_max) {
                    return false;
                }
                let hit_left = left.hit(ray, t_min, t_max, hit_record);
                let new_tmax = if hit_left {hit_record.t} else {t_max};
                let hit_right = right.hit(ray, t_min, new_tmax, hit_record);
                return hit_left || hit_right;
            }
        }
    }
    fn get_bounding_box(&self) -> &AABoundingBox {
        match self {
            BVHNode::Leaf { bounding_box, .. } => &bounding_box,
            BVHNode::Internal { bounding_box, .. } => &bounding_box
        }
    }
}

thread_local! {
    static TRAVERSAL_STACK: RefCell<Vec<usize>> = RefCell::new(Vec::with_capacity(1024));
}

pub enum RayScene {
    BruteForce(Vec<Box<dyn Hittable>>),
    Bvh(FlattenedBVH),
}

impl RayScene {
    #[inline]
    pub fn hit<'a>(&'a self, ray: &Ray, min: f32, mut max: f32, record: &mut HitRecord<'a>) -> bool {
        if max < min {
            return false;
        }
        match self {
            Self::Bvh(tree) => tree.hit(ray, min, max, record),
            Self::BruteForce(objects) => {
                let mut hit = false;
                for object in objects {
                    // Both traversals choose the later scene primitive on an exact tie.
                    if object.hit(ray, min, max, record) {
                        max = record.t;
                        hit = true;
                    }
                }
                hit
            }
        }
    }
}

#[derive(Debug)]
pub struct FlattenedBVH {
    nodes: Vec<FlattenedBVHNode>,
    hittables: Vec<Box<dyn Hittable>>,
}

impl FlattenedBVH {
    pub fn new(hittables: Vec<Box<dyn Hittable>>) -> Self {
        let mut indices: Vec<_> = (0..hittables.len()).collect();
        let mut flattened_bvh = FlattenedBVH {
            nodes: Vec::with_capacity(2 * indices.len()),
            hittables,
        };

        if !indices.is_empty() {
            flattened_bvh.build(&mut indices);
        }
        return flattened_bvh;
    }

    fn build(&mut self, indices: &mut [usize]) -> usize {
        // leaf node
        if indices.len() == 1 {
            self.nodes.push(FlattenedBVHNode::Leaf { hittable_index: indices[0] });
            return self.nodes.len() - 1;
        }

        // internal node
        let mut bounding_box = AABoundingBox::empty();
        for &index in indices.iter() {
            bounding_box = AABoundingBox::from_sub_boxes(&bounding_box, self.hittables[index].get_bounding_box());
        }
        bounding_box.pad_to_minimums();

        let axis = bounding_box.get_longest_axis();
        let center_on_axis = |index: usize| {
            let center = self.hittables[index].get_bounding_box().get_center();
            [center.x, center.y, center.z][axis as usize]
        };
        indices.sort_unstable_by(|&left, &right| {
            center_on_axis(left).total_cmp(&center_on_axis(right)).then(left.cmp(&right))
        });
        
        let (left, right) = indices.split_at_mut(indices.len() / 2);
        let left_child_index = self.build(left);
        let right_child_index = self.build(right);

        let node = FlattenedBVHNode::Internal { 
            bounding_box, 
            left_index: left_child_index, 
            right_index: right_child_index 
        };
        self.nodes.push(node);
        return self.nodes.len() - 1;
    }

    #[inline(always)]
    pub fn hit<'a>(&'a self, ray: &Ray, t_min: f32, mut t_max: f32, hit_record: &mut HitRecord<'a>) -> bool {

        if self.nodes.is_empty() {
            return false;
        }
        let mut winner = None;
        let mut candidate = HitRecord::default();

        TRAVERSAL_STACK.with(|stack_cell| {
            let mut stack = stack_cell.borrow_mut();
            stack.clear();
            stack.push(self.nodes.len() - 1); // start with the root node

            while let Some(node_index) = stack.pop() {
                if node_index >= self.nodes.len() {
                    continue;
                }
                let node = &self.nodes[node_index];
                match node {
                    FlattenedBVHNode::Leaf { hittable_index } => {
                        if self.hittables[*hittable_index].hit(ray, t_min, t_max, &mut candidate)
                            && (candidate.t < t_max || winner.is_none() || Some(*hittable_index) > winner)
                        {
                            t_max = candidate.t;
                            winner = Some(*hittable_index);
                            std::mem::swap(hit_record, &mut candidate);
                        }
                    },
                    FlattenedBVHNode::Internal { bounding_box, left_index, right_index } => {
                        if bounding_box.hit(ray, t_min, t_max) {
                            stack.push(*left_index);
                            stack.push(*right_index);
                        }
                    }
                }
            }

        });

        winner.is_some()
    }
}

#[derive(Debug)]
pub enum FlattenedBVHNode {
    Leaf {
        hittable_index: usize,
    },
    Internal {
        bounding_box: AABoundingBox,
        left_index: usize,
        right_index: usize,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphics::ray_tracing::{hittable::{Sphere, Triangle}, material::{Lambertian, Metal}};

    fn assert_hit(objects: Vec<Box<dyn Hittable>>, ray: Ray, max: f32, expected: Option<(f32, u32)>) {
        let bvh = RayScene::Bvh(FlattenedBVH::new(objects.iter().map(|object| object.clone_box()).collect()));
        let brute = RayScene::BruteForce(objects);
        for scene in [brute, bvh] {
            let mut record = HitRecord::default();
            assert_eq!(scene.hit(&ray, 0.001, max, &mut record), expected.is_some());
            if let Some((t, material)) = expected {
                assert!((record.t - t).abs() < 1e-5, "expected {t}, got {}", record.t);
                assert_eq!(record.material.unwrap().get_material_number(), material);
            }
        }
    }

    #[test]
    fn empty_and_bounded_rays() {
        let ray = Ray::new(Vec3::zero(), Vec3::new(1.0, 0.0, 0.0));
        assert_hit(vec![], ray.clone(), 100.0, None);
        let sphere = Sphere::new(Vec3::new(4.0, 0.0, 0.0), 1.0, Vec3::ones(), Box::new(Lambertian::default()));
        assert_hit(vec![sphere.clone_box()], ray.clone(), 2.999, None);
        assert_hit(vec![sphere.clone_box()], ray, 3.0, Some((3.0, 1)));
    }

    #[test]
    fn transformed_triangle_and_exact_tie() {
        let mut triangle = Triangle::new_from_vertices(
            Vec3::zero(), Vec3::new(2.0, 0.0, 0.0), Vec3::new(0.0, 2.0, 0.0),
            Vec3::ones(), &Lambertian::default(),
        );
        triangle.rotate_around(Vec3::zero(), std::f32::consts::FRAC_PI_2, 0.0);
        triangle.scale_around(Vec3::zero(), 2.0);
        triangle.translate_by(Vec3::new(4.0, -2.0, 3.0));
        let target = triangle.origin + 0.25 * (triangle.u + triangle.v);
        let ray = Ray::new(target + Vec3::new(0.0, 0.0, 5.0), Vec3::new(0.0, 0.0, -1.0));
        let mut tied = triangle.clone();
        tied.set_material(Box::new(Metal::default()));
        let farther = Sphere::new(target - Vec3::new(0.0, 0.0, 3.0), 1.0, Vec3::ones(), Box::new(Lambertian::default()));
        assert_hit(vec![farther.clone_box(), triangle.clone_box(), tied.clone_box()], ray.clone(), 8.0, Some((5.0, 2)));
        assert_hit(vec![tied.clone_box(), farther.clone_box(), triangle.clone_box()], ray.clone(), 8.0, Some((5.0, 1)));
        assert_hit(vec![farther.clone_box(), triangle.clone_box()], ray, 4.99, None);
    }
}
