use margaret_core::color::{ColorRgb, ColorRgba8};
use margaret_core::image::{ImageSize, OutputPixelFormat, RenderMetadata};
use margaret_core::material::{MaterialDescription, MaterialId, MaterialKind};
use margaret_core::math::{Point3, Vec3};
use margaret_core::ray::{HitRecord, Ray};
use margaret_core::render::{RenderDebugMode, RenderMode, RenderSettings};
use margaret_core::scene::{Geometry, SceneDescription, Triangle};
use margaret_image::OwnedImage;

const AIR_REFRACTIVE_INDEX: f32 = 1.0;
const DETERMINANT_EPSILON: f32 = 0.000_1;
const MIN_HIT_DISTANCE: f32 = 0.000_1;
const SHADOW_BIAS: f32 = 0.001;
const MISS_COLOR: ColorRgba8 = ColorRgba8::New(18, 24, 32, 255);
const DEPTH_MISS_COLOR: ColorRgba8 = ColorRgba8::New(0, 0, 0, 255);
const INV_PI: f32 = 0.318_309_87;
const LIT_SAMPLES_PER_PIXEL: u32 = 16;
const MAX_PATH_VERTICES: u32 = 4;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CpuRendererBackend;

impl CpuRendererBackend {
    pub const fn New() -> Self {
        Self
    }

    pub const fn BackendName(&self) -> &'static str {
        "cpu"
    }

    pub fn DescribeRender(
        &self,
        scene: &SceneDescription,
        image_size: ImageSize,
        render_settings: RenderSettings,
    ) -> RenderMetadata {
        ValidateSupportedScene(scene);
        let sample_count = match render_settings.mode {
            RenderMode::Lit => LIT_SAMPLES_PER_PIXEL,
            RenderMode::Debug(_) => 1,
        };

        RenderMetadata {
            BackendName: self.BackendName().to_string(),
            scene_name: scene.name.clone(),
            image_size,
            pixel_format: OutputPixelFormat::Rgba8Unorm,
            sample_count,
            object_count: scene.objects.len(),
            light_count: CountEmissiveTriangles(scene),
        }
    }

    pub fn Render(
        &self,
        scene: &SceneDescription,
        image_size: ImageSize,
        render_settings: RenderSettings,
    ) -> OwnedImage {
        ValidateSupportedScene(scene);
        let mut image = OwnedImage::New(image_size, MissColor(render_settings.mode));
        let emissive_triangles = CollectEmissiveTriangles(scene);

        for pixel_y in 0..image_size.height {
            for pixel_x in 0..image_size.width {
                let color = match render_settings.mode {
                    RenderMode::Lit => {
                        RenderLitPixel(scene, image_size, pixel_x, pixel_y, &emissive_triangles)
                    }
                    RenderMode::Debug(_) => {
                        let ray = scene.camera.RayForPixel(image_size, pixel_x, pixel_y);
                        match ClosestHit(scene, ray) {
                            Some(hit) => {
                                ShadeHit(scene, render_settings, &hit, &emissive_triangles)
                            }
                            None => MissColor(render_settings.mode),
                        }
                    }
                };
                image.SetPixel(pixel_x, pixel_y, color);
            }
        }

        image
    }
}

fn ValidateSupportedScene(scene: &SceneDescription) {
    for material in &scene.materials {
        assert!(
            !material.HasUnsupportedM3aDiffuseEmissionMix(),
            "M3a does not support diffuse materials with both non-black albedo and non-black emission: '{}'",
            material.name,
        );
    }
}

fn RenderLitPixel(
    scene: &SceneDescription,
    image_size: ImageSize,
    pixel_x: u32,
    pixel_y: u32,
    emissive_triangles: &[EmissiveTriangle],
) -> ColorRgba8 {
    let mut rng = PixelRng::New(pixel_x, pixel_y);
    let mut radiance = ColorRgb::BLACK;

    for _sample_index in 0..LIT_SAMPLES_PER_PIXEL {
        let ray =
            scene
                .camera
                .RayForSubpixel(image_size, pixel_x, pixel_y, rng.NextF32(), rng.NextF32());
        radiance += TraceLitPath(
            scene,
            ray,
            emissive_triangles,
            &mut rng,
            MAX_PATH_VERTICES,
            true,
        );
    }

    let average_radiance = radiance * (1.0 / LIT_SAMPLES_PER_PIXEL as f32);
    ColorRgbToRgba8(average_radiance)
}

fn MissColor(render_mode: RenderMode) -> ColorRgba8 {
    match render_mode {
        RenderMode::Debug(RenderDebugMode::Depth) => DEPTH_MISS_COLOR,
        RenderMode::Debug(RenderDebugMode::GeometricNormals)
        | RenderMode::Debug(RenderDebugMode::FlatAlbedo)
        | RenderMode::Lit => MISS_COLOR,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SceneHit {
    pub distance: f32,
    pub position: Point3,
    pub normal: Vec3,
    pub front_face: bool,
    pub material_id: MaterialId,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct EmissiveTriangle {
    pub triangle: Triangle,
    pub radiance: ColorRgb,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PixelRng {
    pub state: u64,
}

impl PixelRng {
    fn New(pixel_x: u32, pixel_y: u32) -> Self {
        let seed = ((pixel_x as u64 + 1) << 32) ^ (pixel_y as u64 + 1) ^ 0x9E37_79B9_7F4A_7C15;
        Self { state: seed }
    }

    fn NextU32(&mut self) -> u32 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.state >> 32) as u32
    }

    fn NextF32(&mut self) -> f32 {
        let value = self.NextU32();
        value as f32 / u32::MAX as f32
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum PathEvent {
    Diffuse {
        albedo: ColorRgb,
        bounce_ray: Ray,
    },
    SpecularReflection {
        reflectance: ColorRgb,
        bounce_ray: Ray,
    },
    Dielectric {
        fresnel_reflectance: f32,
        reflected_ray: Ray,
        refracted_ray: Option<Ray>,
    },
}

fn ShadeHit(
    scene: &SceneDescription,
    render_settings: RenderSettings,
    hit: &SceneHit,
    emissive_triangles: &[EmissiveTriangle],
) -> ColorRgba8 {
    match render_settings.mode {
        RenderMode::Debug(RenderDebugMode::GeometricNormals) => ShadeNormal(hit.normal),
        RenderMode::Debug(RenderDebugMode::FlatAlbedo) => ShadeAlbedo(scene, hit.material_id),
        RenderMode::Debug(RenderDebugMode::Depth) => {
            ShadeDepth(hit.distance, render_settings.depth_max_distance)
        }
        RenderMode::Lit => ColorRgbToRgba8(ShadeLit(scene, hit, emissive_triangles)),
    }
}

fn ShadeNormal(normal: Vec3) -> ColorRgba8 {
    let mapped = (normal + Vec3::New(1.0, 1.0, 1.0)) * 0.5;
    ColorRgba8::New(ToU8(mapped.x), ToU8(mapped.y), ToU8(mapped.z), 255)
}

fn ShadeAlbedo(scene: &SceneDescription, material_id: MaterialId) -> ColorRgba8 {
    let material =
        FindMaterial(scene, material_id).expect("scene hit referenced a missing material");
    ColorRgbToRgba8(material.DiffuseAlbedo())
}

fn ShadeDepth(distance: f32, depth_max_distance: f32) -> ColorRgba8 {
    assert!(
        depth_max_distance > 0.0,
        "depth max distance must be greater than zero"
    );

    let depth = (1.0 - (distance / depth_max_distance)).clamp(0.0, 1.0);
    let channel = ToU8(depth);
    ColorRgba8::New(channel, channel, channel, 255)
}

fn ShadeLit(
    scene: &SceneDescription,
    hit: &SceneHit,
    emissive_triangles: &[EmissiveTriangle],
) -> ColorRgb {
    let material =
        FindMaterial(scene, hit.material_id).expect("scene hit referenced a missing material");
    let mut radiance = VisibleEmissiveRadiance(material, hit.front_face);

    let MaterialKind::Diffuse { albedo, .. } = material.kind else {
        return radiance;
    };

    if material.IsEmissive() {
        return radiance;
    }

    let direct_normal = ScatterNormal(hit);
    for light in emissive_triangles {
        radiance += EvaluateDirectLight(scene, hit.position, direct_normal, albedo, light);
    }

    radiance
}

fn TraceLitPath(
    scene: &SceneDescription,
    ray: Ray,
    emissive_triangles: &[EmissiveTriangle],
    rng: &mut PixelRng,
    remaining_vertices: u32,
    allow_emissive_hit: bool,
) -> ColorRgb {
    if remaining_vertices == 0 {
        return ColorRgb::BLACK;
    }

    let Some(hit) = ClosestHit(scene, ray) else {
        return ColorRgb::BLACK;
    };

    let material =
        FindMaterial(scene, hit.material_id).expect("scene hit referenced a missing material");
    let emitted = if allow_emissive_hit {
        VisibleEmissiveRadiance(material, hit.front_face)
    } else {
        ColorRgb::BLACK
    };

    if material.IsEmissive() {
        return emitted;
    }

    let event = BuildPathEvent(material, ray, &hit, rng);
    let next_remaining_vertices = remaining_vertices - 1;

    match event {
        PathEvent::Diffuse { albedo, bounce_ray } => {
            let mut radiance = emitted;
            let direct_normal = ScatterNormal(&hit);

            for light in emissive_triangles {
                radiance += EvaluateDirectLight(scene, hit.position, direct_normal, albedo, light);
            }

            let indirect = TraceLitPath(
                scene,
                bounce_ray,
                emissive_triangles,
                rng,
                next_remaining_vertices,
                false,
            );

            radiance + albedo * indirect
        }
        PathEvent::SpecularReflection {
            reflectance,
            bounce_ray,
        } => {
            emitted
                + reflectance
                    * TraceLitPath(
                        scene,
                        bounce_ray,
                        emissive_triangles,
                        rng,
                        next_remaining_vertices,
                        true,
                    )
        }
        PathEvent::Dielectric {
            fresnel_reflectance,
            reflected_ray,
            refracted_ray,
        } => {
            let reflected = TraceLitPath(
                scene,
                reflected_ray,
                emissive_triangles,
                rng,
                next_remaining_vertices,
                true,
            ) * fresnel_reflectance;

            let transmitted = match refracted_ray {
                Some(ray) => {
                    TraceLitPath(
                        scene,
                        ray,
                        emissive_triangles,
                        rng,
                        next_remaining_vertices,
                        true,
                    ) * (1.0 - fresnel_reflectance)
                }
                None => ColorRgb::BLACK,
            };

            emitted + reflected + transmitted
        }
    }
}

fn BuildPathEvent(
    material: &MaterialDescription,
    incoming_ray: Ray,
    hit: &SceneHit,
    rng: &mut PixelRng,
) -> PathEvent {
    match material.kind {
        MaterialKind::Diffuse { albedo, .. } => {
            let bounce_normal = ScatterNormal(hit);
            let bounce_direction = SampleCosineWeightedHemisphere(bounce_normal, rng);
            let bounce_origin = OffsetRayOrigin(hit.position, hit.normal, bounce_direction);

            PathEvent::Diffuse {
                albedo,
                bounce_ray: Ray::New(bounce_origin, bounce_direction),
            }
        }
        MaterialKind::SpecularReflector { reflectance } => {
            let bounce_normal = ScatterNormal(hit);
            let reflected_direction = Reflect(incoming_ray.direction.Normalized(), bounce_normal);
            let bounce_origin = OffsetRayOrigin(hit.position, hit.normal, reflected_direction);

            PathEvent::SpecularReflection {
                reflectance,
                bounce_ray: Ray::New(bounce_origin, reflected_direction),
            }
        }
        MaterialKind::Dielectric { refractive_index } => {
            BuildDielectricEvent(incoming_ray, hit, refractive_index)
        }
    }
}

fn BuildDielectricEvent(incoming_ray: Ray, hit: &SceneHit, refractive_index: f32) -> PathEvent {
    let surface_normal = ScatterNormal(hit);
    let unit_direction = incoming_ray.direction.Normalized();
    let reflected_direction = Reflect(unit_direction, surface_normal);
    let reflected_origin = OffsetRayOrigin(hit.position, hit.normal, reflected_direction);
    let (incident_index, transmitted_index) = if hit.front_face {
        (AIR_REFRACTIVE_INDEX, refractive_index)
    } else {
        (refractive_index, AIR_REFRACTIVE_INDEX)
    };
    let eta = incident_index / transmitted_index;
    let cosine = (-unit_direction).Dot(surface_normal).clamp(0.0, 1.0);
    let sin_theta_squared = (1.0 - cosine * cosine).max(0.0);

    if eta * eta * sin_theta_squared > 1.0 {
        return PathEvent::Dielectric {
            fresnel_reflectance: 1.0,
            reflected_ray: Ray::New(reflected_origin, reflected_direction),
            refracted_ray: None,
        };
    }

    let fresnel_reflectance = SchlickFresnel(cosine, incident_index, transmitted_index);
    let refracted_direction = Refract(unit_direction, surface_normal, eta);
    let refracted_origin = OffsetRayOrigin(hit.position, hit.normal, refracted_direction);

    PathEvent::Dielectric {
        fresnel_reflectance,
        reflected_ray: Ray::New(reflected_origin, reflected_direction),
        refracted_ray: Some(Ray::New(refracted_origin, refracted_direction)),
    }
}

fn SampleCosineWeightedHemisphere(normal: Vec3, rng: &mut PixelRng) -> Vec3 {
    let sample_a = rng.NextF32();
    let sample_b = rng.NextF32();
    let radius = sample_a.sqrt();
    let phi = 2.0 * std::f32::consts::PI * sample_b;

    let local_x = radius * phi.cos();
    let local_y = radius * phi.sin();
    let local_z = (1.0 - sample_a).sqrt();

    let tangent = BuildTangent(normal);
    let bitangent = normal.Cross(tangent).Normalized();
    let direction = tangent * local_x + bitangent * local_y + normal * local_z;

    direction.Normalized()
}

fn BuildTangent(normal: Vec3) -> Vec3 {
    let reference = if normal.y.abs() < 0.999 {
        Vec3::Y
    } else {
        Vec3::X
    };

    normal.Cross(reference).Normalized()
}

fn Reflect(direction: Vec3, normal: Vec3) -> Vec3 {
    (direction - normal * (2.0 * direction.Dot(normal))).Normalized()
}

fn Refract(direction: Vec3, normal: Vec3, eta: f32) -> Vec3 {
    let cosine = (-direction).Dot(normal).clamp(0.0, 1.0);
    let perpendicular = (direction + normal * cosine) * eta;
    let parallel = normal * -(1.0 - perpendicular.LengthSquared()).max(0.0).sqrt();

    (perpendicular + parallel).Normalized()
}

fn SchlickFresnel(cosine: f32, incident_index: f32, transmitted_index: f32) -> f32 {
    let ratio = (incident_index - transmitted_index) / (incident_index + transmitted_index);
    let reflectance_at_normal = ratio * ratio;
    reflectance_at_normal + (1.0 - reflectance_at_normal) * (1.0 - cosine).powi(5)
}

fn ScatterNormal(hit: &SceneHit) -> Vec3 {
    if hit.front_face {
        hit.normal
    } else {
        -hit.normal
    }
}

fn OffsetRayOrigin(position: Point3, GeometricNormal: Vec3, direction: Vec3) -> Point3 {
    if direction.Dot(GeometricNormal) >= 0.0 {
        position + GeometricNormal * SHADOW_BIAS
    } else {
        position - GeometricNormal * SHADOW_BIAS
    }
}

fn VisibleEmissiveRadiance(material: &MaterialDescription, front_face: bool) -> ColorRgb {
    if !material.IsEmissive() {
        return ColorRgb::BLACK;
    }

    if front_face {
        material.EmissiveRadiance()
    } else {
        ColorRgb::BLACK
    }
}

fn EvaluateDirectLight(
    scene: &SceneDescription,
    hit_position: Point3,
    hit_normal: Vec3,
    albedo: ColorRgb,
    light: &EmissiveTriangle,
) -> ColorRgb {
    let light_position = light.triangle.Centroid();
    let to_light = light_position - hit_position;
    let distance_squared = to_light.LengthSquared();
    if distance_squared <= SHADOW_BIAS * SHADOW_BIAS {
        return ColorRgb::BLACK;
    }

    let light_direction = to_light.Normalized();
    let surface_cosine = hit_normal.Dot(light_direction);
    if surface_cosine <= 0.0 {
        return ColorRgb::BLACK;
    }

    let light_normal = light.triangle.GeometricNormal();
    let light_cosine = light_normal.Dot(-light_direction);
    if light_cosine <= 0.0 {
        return ColorRgb::BLACK;
    }

    let shadow_origin = OffsetRayOrigin(hit_position, hit_normal, light_direction);
    let shadow_distance = (light_position - shadow_origin).Length();
    let shadow_ray = Ray::New(shadow_origin, light_direction);

    if IsOccluded(scene, shadow_ray, shadow_distance - SHADOW_BIAS) {
        return ColorRgb::BLACK;
    }

    let geometry_term = (surface_cosine * light_cosine * light.triangle.Area()) / distance_squared;
    let brdf = albedo * INV_PI;
    brdf * light.radiance * geometry_term
}

fn IsOccluded(scene: &SceneDescription, ray: Ray, max_distance: f32) -> bool {
    TraceHit(scene, ray, MIN_HIT_DISTANCE, max_distance).is_some()
}

fn ClosestHit(scene: &SceneDescription, ray: Ray) -> Option<SceneHit> {
    let hit = TraceHit(scene, ray, MIN_HIT_DISTANCE, f32::INFINITY)?;

    Some(SceneHit {
        distance: hit.distance,
        position: hit.position,
        normal: hit.normal,
        front_face: hit.front_face,
        material_id: hit.material_id,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TraceHit {
    pub distance: f32,
    pub position: Point3,
    pub normal: Vec3,
    pub front_face: bool,
    pub material_id: MaterialId,
}

fn TraceHit(scene: &SceneDescription, ray: Ray, t_min: f32, t_max: f32) -> Option<TraceHit> {
    let mut ClosestHit = None;
    let mut closest_distance = t_max;

    for object in &scene.objects {
        match &object.geometry {
            Geometry::TriangleMesh { triangles } => {
                for triangle in triangles {
                    let hit = IntersectTriangle(ray, triangle, t_min, closest_distance);
                    if let Some(hit) = hit {
                        closest_distance = hit.distance;
                        ClosestHit = Some(TraceHit {
                            distance: hit.distance,
                            position: hit.position,
                            normal: hit.normal,
                            front_face: hit.front_face,
                            material_id: object.material_id,
                        });
                    }
                }
            }
        }
    }

    ClosestHit
}

fn CollectEmissiveTriangles(scene: &SceneDescription) -> Vec<EmissiveTriangle> {
    let mut lights = Vec::new();

    for object in &scene.objects {
        let material = FindMaterial(scene, object.material_id)
            .expect("scene object referenced a missing material");
        if !material.IsEmissive() {
            continue;
        }

        match &object.geometry {
            Geometry::TriangleMesh { triangles } => {
                for triangle in triangles {
                    lights.push(EmissiveTriangle {
                        triangle: *triangle,
                        radiance: material.EmissiveRadiance(),
                    });
                }
            }
        }
    }

    lights
}

fn CountEmissiveTriangles(scene: &SceneDescription) -> usize {
    CollectEmissiveTriangles(scene).len()
}

fn IntersectTriangle(ray: Ray, triangle: &Triangle, t_min: f32, t_max: f32) -> Option<HitRecord> {
    let vertex0 = triangle.vertices[0];
    let vertex1 = triangle.vertices[1];
    let vertex2 = triangle.vertices[2];

    let edge1 = vertex1 - vertex0;
    let edge2 = vertex2 - vertex0;
    let pvec = ray.direction.Cross(edge2);
    let determinant = edge1.Dot(pvec);

    if determinant.abs() < DETERMINANT_EPSILON {
        return None;
    }

    let inverse_determinant = 1.0 / determinant;
    let tvec = ray.origin - vertex0;
    let u = tvec.Dot(pvec) * inverse_determinant;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }

    let qvec = tvec.Cross(edge1);
    let v = ray.direction.Dot(qvec) * inverse_determinant;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }

    let distance = edge2.Dot(qvec) * inverse_determinant;
    if distance < t_min || distance > t_max {
        return None;
    }

    let normal = triangle.GeometricNormal();
    let front_face = ray.direction.Dot(normal) < 0.0;

    Some(HitRecord {
        distance,
        position: ray.At(distance),
        normal,
        front_face,
        triangle_index: 0,
    })
}

fn ColorRgbToRgba8(color: ColorRgb) -> ColorRgba8 {
    ColorRgba8::New(ToU8(color.r), ToU8(color.g), ToU8(color.b), 255)
}

fn FindMaterial(scene: &SceneDescription, material_id: MaterialId) -> Option<&MaterialDescription> {
    scene
        .materials
        .iter()
        .find(|material| material.id == material_id)
}

fn ToU8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::{
        BuildDielectricEvent, BuildPathEvent, ClosestHit, ColorRgbToRgba8, CpuRendererBackend,
        DEPTH_MISS_COLOR, FindMaterial, IntersectTriangle, LIT_SAMPLES_PER_PIXEL,
        MAX_PATH_VERTICES, MIN_HIT_DISTANCE, MISS_COLOR, MissColor, PathEvent, PixelRng, Reflect,
        SampleCosineWeightedHemisphere, SceneHit, SchlickFresnel, TraceLitPath,
        ValidateSupportedScene,
    };
    use margaret_core::camera::Camera;
    use margaret_core::color::{ColorRgb, ColorRgba8};
    use margaret_core::image::ImageSize;
    use margaret_core::material::{MaterialDescription, MaterialId, MaterialKind};
    use margaret_core::math::{Point3, Vec3};
    use margaret_core::ray::Ray;
    use margaret_core::render::{RenderDebugMode, RenderMode, RenderSettings};
    use margaret_core::scene::{Geometry, SceneDescription, SceneObject, Triangle};
    use margaret_testutil::SampleImageSize;

    #[test]
    fn describe_render_reports_basic_scene_counts() {
        let backend = CpuRendererBackend::New();
        let metadata = backend.DescribeRender(
            &LitRoomScene(),
            SampleImageSize(),
            RenderSettings::New(RenderMode::Lit, 6.0),
        );

        assert_eq!(metadata.BackendName, "cpu");
        assert_eq!(metadata.object_count, 7);
        assert_eq!(metadata.light_count, 2);
        assert_eq!(metadata.sample_count, LIT_SAMPLES_PER_PIXEL);
        assert_eq!(MAX_PATH_VERTICES, 4);
    }

    #[test]
    fn ray_triangle_intersection_returns_expected_distance() {
        let ray = Ray::New(Point3::New(0.0, 0.0, 1.0), Vec3::New(0.0, 0.0, -1.0));
        let triangle = Triangle::New(
            Point3::New(-1.0, -1.0, 0.0),
            Point3::New(1.0, -1.0, 0.0),
            Point3::New(0.0, 1.0, 0.0),
        );

        let hit = IntersectTriangle(ray, &triangle, 0.001, f32::INFINITY).unwrap();

        assert!((hit.distance - 1.0).abs() < 0.0001);
        assert_eq!(hit.position, Point3::New(0.0, 0.0, 0.0));
        assert_eq!(hit.normal, Vec3::New(0.0, 0.0, 1.0));
    }

    #[test]
    fn ray_triangle_intersection_rejects_miss() {
        let ray = Ray::New(Point3::New(2.0, 2.0, 1.0), Vec3::New(0.0, 0.0, -1.0));
        let triangle = Triangle::New(
            Point3::New(-1.0, -1.0, 0.0),
            Point3::New(1.0, -1.0, 0.0),
            Point3::New(0.0, 1.0, 0.0),
        );

        let hit = IntersectTriangle(ray, &triangle, 0.001, f32::INFINITY);

        assert!(hit.is_none());
    }

    #[test]
    fn ray_triangle_intersection_keeps_geometric_normal_for_backface_hits() {
        let ray = Ray::New(Point3::New(0.0, 0.0, -1.0), Vec3::New(0.0, 0.0, 1.0));
        let triangle = Triangle::New(
            Point3::New(-1.0, -1.0, 0.0),
            Point3::New(1.0, -1.0, 0.0),
            Point3::New(0.0, 1.0, 0.0),
        );

        let hit = IntersectTriangle(ray, &triangle, MIN_HIT_DISTANCE, f32::INFINITY).unwrap();

        assert_eq!(hit.normal, Vec3::New(0.0, 0.0, 1.0));
        assert!(!hit.front_face);
    }

    #[test]
    fn closest_hit_prefers_nearest_triangle() {
        let mut scene = LitRoomScene();
        scene.objects[0].geometry = Geometry::TriangleMesh {
            triangles: vec![
                Triangle::New(
                    Point3::New(-0.5, -0.5, 0.0),
                    Point3::New(0.5, -0.5, 0.0),
                    Point3::New(0.0, 0.5, 0.0),
                ),
                Triangle::New(
                    Point3::New(-0.5, -0.5, -1.0),
                    Point3::New(0.5, -0.5, -1.0),
                    Point3::New(0.0, 0.5, -1.0),
                ),
            ],
        };
        let ray = Ray::New(Point3::New(0.0, 0.0, 1.0), Vec3::New(0.0, 0.0, -1.0));

        let hit = ClosestHit(&scene, ray).unwrap();

        assert!((hit.distance - 1.0).abs() < 0.0001);
        assert_eq!(hit.material_id, MaterialId(2));
    }

    #[test]
    fn flat_albedo_mode_returns_material_color() {
        let backend = CpuRendererBackend::New();
        let image = backend.Render(
            &LitRoomScene(),
            ImageSize::New(5, 5),
            RenderSettings::New(RenderMode::Debug(RenderDebugMode::FlatAlbedo), 6.0),
        );

        assert_eq!(image.GetPixel(2, 2), ColorRgba8::New(204, 204, 204, 255));
    }

    #[test]
    fn normals_mode_returns_mapped_normal_color() {
        let backend = CpuRendererBackend::New();
        let image = backend.Render(
            &SingleTriangleScene(),
            ImageSize::New(3, 3),
            RenderSettings::New(RenderMode::Debug(RenderDebugMode::GeometricNormals), 6.0),
        );

        assert_eq!(image.GetPixel(1, 1), ColorRgba8::New(128, 128, 255, 255));
    }

    #[test]
    fn depth_mode_brightens_nearer_hits_and_keeps_misses_dark() {
        let backend = CpuRendererBackend::New();
        let image = backend.Render(
            &SingleTriangleScene(),
            ImageSize::New(5, 5),
            RenderSettings::New(RenderMode::Debug(RenderDebugMode::Depth), 6.0),
        );

        assert_eq!(image.GetPixel(0, 0), DEPTH_MISS_COLOR);

        let center = image.GetPixel(2, 2);
        assert_eq!(center.r, center.g);
        assert_eq!(center.g, center.b);
        assert!(center.r > 0);
    }

    #[test]
    fn lit_mode_receives_emissive_triangle_contribution() {
        let scene = SimpleLightingScene(false);
        let hit = SceneHit {
            distance: 3.0,
            position: Point3::New(0.0, 0.0, 0.0),
            normal: Vec3::New(0.0, 0.0, 1.0),
            front_face: true,
            material_id: MaterialId(0),
        };
        let lights = super::CollectEmissiveTriangles(&scene);
        let color = super::ShadeLit(&scene, &hit, &lights);

        assert!(color.r > 0.0);
        assert!(color.g > 0.0);
        assert!(color.b > 0.0);
    }

    #[test]
    fn lit_mode_returns_shadow_when_occluder_blocks_light() {
        let lit_scene = SimpleLightingScene(false);
        let shadowed_scene = SimpleLightingScene(true);
        let hit = SceneHit {
            distance: 3.0,
            position: Point3::New(0.0, 0.0, 0.0),
            normal: Vec3::New(0.0, 0.0, 1.0),
            front_face: true,
            material_id: MaterialId(0),
        };

        let lit_lights = super::CollectEmissiveTriangles(&lit_scene);
        let shadowed_lights = super::CollectEmissiveTriangles(&shadowed_scene);
        let lit_color = super::ShadeLit(&lit_scene, &hit, &lit_lights);
        let shadowed_color = super::ShadeLit(&shadowed_scene, &hit, &shadowed_lights);

        assert!(lit_color.r > shadowed_color.r);
        assert!(lit_color.g > shadowed_color.g);
        assert!(lit_color.b > shadowed_color.b);
    }

    #[test]
    fn primary_camera_ray_sees_front_face_emission() {
        let scene = EmissiveTriangleScene();
        let lights = super::CollectEmissiveTriangles(&scene);
        let ray = Ray::New(Point3::New(0.0, 0.0, 1.0), Vec3::New(0.0, 0.0, -1.0));
        let mut rng = PixelRng::New(1, 2);

        let color = TraceLitPath(&scene, ray, &lights, &mut rng, MAX_PATH_VERTICES, true);

        assert!(color.r > 0.0);
        assert!(color.g > 0.0);
        assert!(color.b > 0.0);
    }

    #[test]
    fn primary_camera_ray_rejects_back_face_emission() {
        let scene = EmissiveTriangleScene();
        let lights = super::CollectEmissiveTriangles(&scene);
        let ray = Ray::New(Point3::New(0.0, 0.0, -1.0), Vec3::New(0.0, 0.0, 1.0));
        let mut rng = PixelRng::New(3, 4);

        let color = TraceLitPath(&scene, ray, &lights, &mut rng, MAX_PATH_VERTICES, true);

        assert_eq!(color, ColorRgb::BLACK);
    }

    #[test]
    fn direct_light_stays_black_when_first_hit_cannot_see_emitter() {
        let scene = IndirectBounceScene();
        let lights = super::CollectEmissiveTriangles(&scene);
        let hit = SceneHit {
            distance: 1.5,
            position: Point3::New(0.0, 0.0, 0.0),
            normal: Vec3::New(0.0, 0.0, 1.0),
            front_face: true,
            material_id: MaterialId(0),
        };

        let color = super::ShadeLit(&scene, &hit, &lights);

        assert_eq!(color, ColorRgb::BLACK);
    }

    #[test]
    fn path_trace_adds_indirect_bounce_when_light_is_hidden_from_first_hit() {
        let scene = IndirectBounceScene();
        let lights = super::CollectEmissiveTriangles(&scene);
        let ray = Ray::New(Point3::New(0.0, 0.0, 1.5), Vec3::New(0.0, 0.0, -1.0));
        let mut rng = PixelRng::New(2, 3);
        let mut color = ColorRgb::BLACK;

        for _sample_index in 0..256 {
            color += TraceLitPath(&scene, ray, &lights, &mut rng, MAX_PATH_VERTICES, true);
        }

        assert!(color.r > 0.0);
        assert!(color.g > 0.0);
        assert!(color.b > 0.0);
    }

    #[test]
    fn path_trace_does_not_double_count_direct_emitter_hits_after_diffuse_bounce() {
        let scene = DirectLightRegressionScene();
        let lights = super::CollectEmissiveTriangles(&scene);
        let ray = Ray::New(Point3::New(0.0, 0.0, 2.0), Vec3::New(0.0, 0.0, -1.0));
        let hit = SceneHit {
            distance: 2.0,
            position: Point3::New(0.0, 0.0, 0.0),
            normal: Vec3::New(0.0, 0.0, 1.0),
            front_face: true,
            material_id: MaterialId(0),
        };
        let expected_direct = super::ShadeLit(&scene, &hit, &lights);
        let mut rng = PixelRng::New(5, 6);

        let color = TraceLitPath(&scene, ray, &lights, &mut rng, MAX_PATH_VERTICES, true);

        AssertColorNear(color, expected_direct, 0.0001);
    }

    #[test]
    fn mirror_path_sees_emitter_after_reflection() {
        let scene = MirrorReflectionScene();
        let lights = super::CollectEmissiveTriangles(&scene);
        let ray = Ray::New(Point3::New(0.0, 0.0, 1.0), Vec3::New(0.0, 0.0, -1.0));
        let mut rng = PixelRng::New(8, 9);

        let color = TraceLitPath(&scene, ray, &lights, &mut rng, MAX_PATH_VERTICES, true);

        assert!(color.r > 0.0);
        assert!(color.g > 0.0);
        assert!(color.b > 0.0);
    }

    #[test]
    fn mirror_does_not_receive_diffuse_direct_light_estimate() {
        let scene = MirrorDirectLightRegressionScene();
        let lights = super::CollectEmissiveTriangles(&scene);
        let ray = Ray::New(Point3::New(0.0, 0.0, 1.0), Vec3::New(0.0, 0.0, -1.0));
        let mut rng = PixelRng::New(10, 11);

        let color = TraceLitPath(&scene, ray, &lights, &mut rng, MAX_PATH_VERTICES, true);

        assert_eq!(color, ColorRgb::BLACK);
    }

    #[test]
    fn dielectric_path_transmits_emitter_at_normal_incidence() {
        let scene = GlassTransmissionScene();
        let lights = super::CollectEmissiveTriangles(&scene);
        let ray = Ray::New(Point3::New(0.0, 0.0, 1.0), Vec3::New(0.0, 0.0, -1.0));
        let mut rng = PixelRng::New(12, 13);

        let color = TraceLitPath(&scene, ray, &lights, &mut rng, MAX_PATH_VERTICES, true);

        assert!(color.r > 0.0);
        assert!(color.g > 0.0);
        assert!(color.b > 0.0);
    }

    #[test]
    fn dielectric_total_internal_reflection_returns_reflection_only() {
        let hit = SceneHit {
            distance: 1.0,
            position: Point3::ORIGIN,
            normal: Vec3::New(0.0, 0.0, 1.0),
            front_face: false,
            material_id: MaterialId(0),
        };
        let ray = Ray::New(
            Point3::New(0.0, 0.0, -0.5),
            Vec3::New(0.9, 0.0, 0.435_889_9).Normalized(),
        );

        let event = BuildDielectricEvent(ray, &hit, 1.5);

        let PathEvent::Dielectric {
            fresnel_reflectance,
            refracted_ray,
            ..
        } = event
        else {
            panic!("expected dielectric event");
        };

        assert_eq!(fresnel_reflectance, 1.0);
        assert!(refracted_ray.is_none());
    }

    #[test]
    fn dielectric_front_face_refraction_uses_air_to_glass_eta() {
        let hit = SceneHit {
            distance: 1.0,
            position: Point3::ORIGIN,
            normal: Vec3::New(0.0, 0.0, 1.0),
            front_face: true,
            material_id: MaterialId(0),
        };
        let ray = Ray::New(
            Point3::New(0.0, 0.0, 1.0),
            Vec3::New(0.707_106_77, 0.0, -0.707_106_77),
        );

        let event = BuildDielectricEvent(ray, &hit, 1.5);

        let PathEvent::Dielectric {
            fresnel_reflectance,
            reflected_ray,
            refracted_ray,
        } = event
        else {
            panic!("expected dielectric event");
        };

        assert!((fresnel_reflectance - 0.042_069_27).abs() <= 0.000_001);
        AssertVec3Near(
            reflected_ray.direction,
            Vec3::New(0.707_106_77, 0.0, 0.707_106_77),
            0.000_001,
        );

        let refracted_ray = refracted_ray.expect("expected transmitted ray");
        AssertVec3Near(
            refracted_ray.direction,
            Vec3::New(0.471_404_55, 0.0, -0.881_917_1),
            0.000_001,
        );
    }

    #[test]
    fn dielectric_back_face_refraction_uses_glass_to_air_eta() {
        let hit = SceneHit {
            distance: 1.0,
            position: Point3::ORIGIN,
            normal: Vec3::New(0.0, 0.0, 1.0),
            front_face: false,
            material_id: MaterialId(0),
        };
        let ray = Ray::New(
            Point3::New(0.0, 0.0, -1.0),
            Vec3::New(0.5, 0.0, 0.866_025_4),
        );

        let event = BuildDielectricEvent(ray, &hit, 1.5);

        let PathEvent::Dielectric {
            fresnel_reflectance,
            reflected_ray,
            refracted_ray,
        } = event
        else {
            panic!("expected dielectric event");
        };

        assert!((fresnel_reflectance - 0.040_041_436).abs() <= 0.000_001);
        AssertVec3Near(
            reflected_ray.direction,
            Vec3::New(0.5, 0.0, -0.866_025_4),
            0.000_001,
        );

        let refracted_ray = refracted_ray.expect("expected transmitted ray");
        AssertVec3Near(
            refracted_ray.direction,
            Vec3::New(0.75, 0.0, 0.661_437_8),
            0.000_001,
        );
    }

    #[test]
    fn dielectric_transmission_keeps_emissive_hits_visible_on_delta_paths() {
        let scene = GlassTransmissionScene();
        let lights = super::CollectEmissiveTriangles(&scene);
        let ray = Ray::New(Point3::New(0.0, 0.0, 1.0), Vec3::New(0.0, 0.0, -1.0));
        let mut rng = PixelRng::New(12, 13);

        let color = TraceLitPath(&scene, ray, &lights, &mut rng, MAX_PATH_VERTICES, true);

        AssertColorNear(color, ColorRgb::New(2.88, 2.4, 1.92), 0.000_001);
    }

    #[test]
    fn build_path_event_reflects_mirror_direction() {
        let material = MaterialDescription::New(
            MaterialId(0),
            "mirror",
            MaterialKind::SpecularReflector {
                reflectance: ColorRgb::WHITE,
            },
        );
        let ray = Ray::New(
            Point3::New(0.0, 0.0, 1.0),
            Vec3::New(0.0, -1.0, -1.0).Normalized(),
        );
        let hit = SceneHit {
            distance: 1.0,
            position: Point3::ORIGIN,
            normal: Vec3::New(0.0, 0.0, 1.0),
            front_face: true,
            material_id: MaterialId(0),
        };
        let mut rng = PixelRng::New(0, 0);

        let event = BuildPathEvent(&material, ray, &hit, &mut rng);

        let PathEvent::SpecularReflection { bounce_ray, .. } = event else {
            panic!("expected mirror event");
        };

        AssertVec3Near(
            bounce_ray.direction,
            Vec3::New(0.0, -1.0, 1.0).Normalized(),
            0.000_001,
        );
    }

    #[test]
    fn schlick_fresnel_increases_toward_grazing_angles() {
        let near_normal = SchlickFresnel(1.0, 1.0, 1.5);
        let grazing = SchlickFresnel(0.1, 1.0, 1.5);

        assert!(grazing > near_normal);
    }

    #[test]
    fn reflect_flips_direction_about_surface_normal() {
        let direction = Vec3::New(0.0, -1.0, -1.0).Normalized();
        let normal = Vec3::New(0.0, 0.0, 1.0);
        let reflected = Reflect(direction, normal);

        AssertVec3Near(reflected, Vec3::New(0.0, -1.0, 1.0).Normalized(), 0.000_001);
    }

    #[test]
    fn cosine_weighted_samples_stay_in_surface_hemisphere() {
        let normal = Vec3::New(0.0, 1.0, 0.0);
        let mut rng = PixelRng::New(4, 5);

        for _ in 0..32 {
            let direction = SampleCosineWeightedHemisphere(normal, &mut rng);
            assert!(direction.Dot(normal) > 0.0);
        }
    }

    #[test]
    fn miss_color_matches_render_mode() {
        assert_eq!(MissColor(RenderMode::Lit), MISS_COLOR);
        assert_eq!(
            MissColor(RenderMode::Debug(RenderDebugMode::Depth)),
            DEPTH_MISS_COLOR
        );
    }

    #[test]
    fn color_conversion_clamps_and_scales() {
        let color = ColorRgb::New(1.2, 0.5, -0.2);

        assert_eq!(ColorRgbToRgba8(color), ColorRgba8::New(255, 128, 0, 255));
    }

    #[test]
    fn find_material_returns_none_for_missing_material() {
        let scene = LitRoomScene();

        assert!(FindMaterial(&scene, MaterialId(99)).is_none());
    }

    #[test]
    #[should_panic(
        expected = "M3a does not support diffuse materials with both non-black albedo and non-black emission"
    )]
    fn ValidateSupportedSceneRejectsMixedDiffuseEmissionMaterials() {
        let camera = Camera::New(
            "main",
            Point3::New(0.0, 0.0, 2.0),
            Vec3::New(0.0, 0.0, -1.0),
            Vec3::Y,
            45.0,
        );
        let material_id = MaterialId(0);

        let mut scene = SceneDescription::New("unsupported-mixed-emission", camera);
        scene.materials.push(MakeDiffuse(
            material_id,
            "mixed-light",
            ColorRgb::New(0.5, 0.5, 0.5),
            ColorRgb::New(2.0, 2.0, 2.0),
        ));

        ValidateSupportedScene(&scene);
    }

    fn SingleTriangleScene() -> SceneDescription {
        let camera = Camera::New(
            "main",
            Point3::New(0.0, 0.0, 2.0),
            Vec3::New(0.0, 0.0, -1.0),
            Vec3::Y,
            45.0,
        );
        let material_id = MaterialId(0);

        let mut scene = SceneDescription::New("single-triangle", camera);
        scene.materials.push(MakeDiffuse(
            material_id,
            "gray",
            ColorRgb::New(0.6, 0.6, 0.6),
            ColorRgb::BLACK,
        ));
        scene.objects.push(SceneObject::New(
            "triangle",
            Geometry::TriangleMesh {
                triangles: vec![Triangle::New(
                    Point3::New(-1.0, -1.0, 0.0),
                    Point3::New(1.0, -1.0, 0.0),
                    Point3::New(0.0, 1.0, 0.0),
                )],
            },
            material_id,
        ));

        scene
    }

    fn EmissiveTriangleScene() -> SceneDescription {
        let camera = Camera::New(
            "main",
            Point3::New(0.0, 0.0, 1.0),
            Vec3::New(0.0, 0.0, -1.0),
            Vec3::Y,
            45.0,
        );
        let light = MaterialId(0);

        let mut scene = SceneDescription::New("emissive-triangle", camera);
        scene.materials.push(MakeDiffuse(
            light,
            "light",
            ColorRgb::BLACK,
            ColorRgb::New(3.0, 2.0, 1.0),
        ));
        scene.objects.push(SceneObject::New(
            "light",
            Geometry::TriangleMesh {
                triangles: vec![Triangle::New(
                    Point3::New(-1.0, -1.0, 0.0),
                    Point3::New(1.0, -1.0, 0.0),
                    Point3::New(0.0, 1.0, 0.0),
                )],
            },
            light,
        ));

        scene
    }

    fn DirectLightRegressionScene() -> SceneDescription {
        let camera = Camera::New(
            "main",
            Point3::New(0.0, 0.0, 2.0),
            Vec3::New(0.0, 0.0, -1.0),
            Vec3::Y,
            45.0,
        );
        let receiver = MaterialId(0);
        let light = MaterialId(1);

        let mut scene = SceneDescription::New("direct-light-regression", camera);
        scene.materials.push(MakeDiffuse(
            receiver,
            "receiver",
            ColorRgb::New(0.8, 0.8, 0.8),
            ColorRgb::BLACK,
        ));
        scene.materials.push(MakeDiffuse(
            light,
            "light",
            ColorRgb::BLACK,
            ColorRgb::New(4.0, 4.0, 4.0),
        ));

        scene.objects.push(SceneObject::New(
            "receiver",
            Geometry::TriangleMesh {
                triangles: vec![Triangle::New(
                    Point3::New(-1.0, -1.0, 0.0),
                    Point3::New(1.0, -1.0, 0.0),
                    Point3::New(0.0, 1.0, 0.0),
                )],
            },
            receiver,
        ));
        scene.objects.push(SceneObject::New(
            "light",
            Geometry::TriangleMesh {
                triangles: vec![
                    Triangle::New(
                        Point3::New(-100.0, -100.0, 1.0),
                        Point3::New(100.0, 100.0, 1.0),
                        Point3::New(100.0, -100.0, 1.0),
                    ),
                    Triangle::New(
                        Point3::New(-100.0, -100.0, 1.0),
                        Point3::New(-100.0, 100.0, 1.0),
                        Point3::New(100.0, 100.0, 1.0),
                    ),
                ],
            },
            light,
        ));

        scene
    }

    fn SimpleLightingScene(with_occluder: bool) -> SceneDescription {
        let camera = Camera::New(
            "main",
            Point3::New(0.0, 0.0, 2.0),
            Vec3::New(0.0, 0.0, -1.0),
            Vec3::Y,
            45.0,
        );

        let matte = MaterialId(0);
        let light = MaterialId(1);
        let occluder = MaterialId(2);

        let mut scene = SceneDescription::New("simple-lighting", camera);
        scene.materials.push(MakeDiffuse(
            matte,
            "matte",
            ColorRgb::New(0.8, 0.8, 0.8),
            ColorRgb::BLACK,
        ));
        scene.materials.push(MakeDiffuse(
            light,
            "light",
            ColorRgb::BLACK,
            ColorRgb::New(4.0, 4.0, 4.0),
        ));
        scene.materials.push(MakeDiffuse(
            occluder,
            "occluder",
            ColorRgb::New(0.2, 0.2, 0.8),
            ColorRgb::BLACK,
        ));

        scene.objects.push(SceneObject::New(
            "receiver",
            Geometry::TriangleMesh {
                triangles: vec![Triangle::New(
                    Point3::New(-1.0, -1.0, 0.0),
                    Point3::New(1.0, -1.0, 0.0),
                    Point3::New(0.0, 1.0, 0.0),
                )],
            },
            matte,
        ));

        scene.objects.push(SceneObject::New(
            "light",
            Geometry::TriangleMesh {
                triangles: vec![Triangle::New(
                    Point3::New(-0.4, 0.4, 1.0),
                    Point3::New(0.4, 0.4, 1.0),
                    Point3::New(0.0, -0.4, 1.0),
                )],
            },
            light,
        ));

        if with_occluder {
            scene.objects.push(SceneObject::New(
                "occluder",
                Geometry::TriangleMesh {
                    triangles: vec![Triangle::New(
                        Point3::New(-0.2, -0.2, 0.5),
                        Point3::New(0.2, -0.2, 0.5),
                        Point3::New(0.0, 0.3, 0.5),
                    )],
                },
                occluder,
            ));
        }

        scene
    }

    fn IndirectBounceScene() -> SceneDescription {
        let camera = Camera::New(
            "main",
            Point3::New(0.0, 0.0, 1.5),
            Vec3::New(0.0, 0.0, -1.0),
            Vec3::Y,
            45.0,
        );
        let receiver = MaterialId(0);
        let bounce = MaterialId(1);
        let light = MaterialId(2);
        let blocker = MaterialId(3);

        let mut scene = SceneDescription::New("indirect-bounce", camera);
        scene.materials.push(MakeDiffuse(
            receiver,
            "receiver",
            ColorRgb::New(0.8, 0.8, 0.8),
            ColorRgb::BLACK,
        ));
        scene.materials.push(MakeDiffuse(
            bounce,
            "bounce",
            ColorRgb::New(0.8, 0.2, 0.2),
            ColorRgb::BLACK,
        ));
        scene.materials.push(MakeDiffuse(
            light,
            "light",
            ColorRgb::BLACK,
            ColorRgb::New(5.0, 5.0, 5.0),
        ));
        scene.materials.push(MakeDiffuse(
            blocker,
            "blocker",
            ColorRgb::New(0.7, 0.7, 0.7),
            ColorRgb::BLACK,
        ));

        scene.objects.push(SceneObject::New(
            "receiver",
            Geometry::TriangleMesh {
                triangles: vec![Triangle::New(
                    Point3::New(-0.8, -0.8, 0.0),
                    Point3::New(0.8, -0.8, 0.0),
                    Point3::New(0.0, 0.8, 0.0),
                )],
            },
            receiver,
        ));

        scene.objects.push(SceneObject::New(
            "blocker",
            Geometry::TriangleMesh {
                triangles: vec![
                    Triangle::New(
                        Point3::New(-0.25, -0.25, 0.25),
                        Point3::New(0.25, -0.25, 0.25),
                        Point3::New(0.25, 0.25, 0.25),
                    ),
                    Triangle::New(
                        Point3::New(-0.25, -0.25, 0.25),
                        Point3::New(0.25, 0.25, 0.25),
                        Point3::New(-0.25, 0.25, 0.25),
                    ),
                ],
            },
            blocker,
        ));

        scene.objects.push(SceneObject::New(
            "bounce-wall",
            Geometry::TriangleMesh {
                triangles: vec![
                    Triangle::New(
                        Point3::New(0.9, -0.7, 0.8),
                        Point3::New(0.9, -0.7, -0.4),
                        Point3::New(0.9, 0.7, -0.4),
                    ),
                    Triangle::New(
                        Point3::New(0.9, -0.7, 0.8),
                        Point3::New(0.9, 0.7, -0.4),
                        Point3::New(0.9, 0.7, 0.8),
                    ),
                ],
            },
            bounce,
        ));

        scene.objects.push(SceneObject::New(
            "light",
            Geometry::TriangleMesh {
                triangles: vec![
                    Triangle::New(
                        Point3::New(0.6, -0.2, 0.75),
                        Point3::New(1.1, -0.2, 0.75),
                        Point3::New(1.1, 0.2, 0.75),
                    ),
                    Triangle::New(
                        Point3::New(0.6, -0.2, 0.75),
                        Point3::New(1.1, 0.2, 0.75),
                        Point3::New(0.6, 0.2, 0.75),
                    ),
                ],
            },
            light,
        ));

        scene
    }

    fn MirrorReflectionScene() -> SceneDescription {
        let camera = Camera::New(
            "main",
            Point3::New(0.0, 0.0, 1.0),
            Vec3::New(0.0, 0.0, -1.0),
            Vec3::Y,
            45.0,
        );
        let mirror = MaterialId(0);
        let light = MaterialId(1);

        let mut scene = SceneDescription::New("mirror-reflection", camera);
        scene.materials.push(MaterialDescription::New(
            mirror,
            "mirror",
            MaterialKind::SpecularReflector {
                reflectance: ColorRgb::WHITE,
            },
        ));
        scene.materials.push(MakeDiffuse(
            light,
            "light",
            ColorRgb::BLACK,
            ColorRgb::New(3.0, 3.0, 3.0),
        ));

        scene.objects.push(SceneObject::New(
            "mirror",
            Geometry::TriangleMesh {
                triangles: vec![
                    Triangle::New(
                        Point3::New(-1.0, -1.0, 0.0),
                        Point3::New(1.0, -1.0, 0.0),
                        Point3::New(1.0, 1.0, 0.0),
                    ),
                    Triangle::New(
                        Point3::New(-1.0, -1.0, 0.0),
                        Point3::New(1.0, 1.0, 0.0),
                        Point3::New(-1.0, 1.0, 0.0),
                    ),
                ],
            },
            mirror,
        ));
        scene.objects.push(SceneObject::New(
            "light",
            Geometry::TriangleMesh {
                triangles: vec![
                    Triangle::New(
                        Point3::New(-1.0, -1.0, 2.0),
                        Point3::New(1.0, 1.0, 2.0),
                        Point3::New(1.0, -1.0, 2.0),
                    ),
                    Triangle::New(
                        Point3::New(-1.0, -1.0, 2.0),
                        Point3::New(-1.0, 1.0, 2.0),
                        Point3::New(1.0, 1.0, 2.0),
                    ),
                ],
            },
            light,
        ));

        scene
    }

    fn MirrorDirectLightRegressionScene() -> SceneDescription {
        let camera = Camera::New(
            "main",
            Point3::New(0.0, 0.0, 1.0),
            Vec3::New(0.0, 0.0, -1.0),
            Vec3::Y,
            45.0,
        );
        let mirror = MaterialId(0);
        let light = MaterialId(1);

        let mut scene = SceneDescription::New("mirror-direct-light-regression", camera);
        scene.materials.push(MaterialDescription::New(
            mirror,
            "mirror",
            MaterialKind::SpecularReflector {
                reflectance: ColorRgb::WHITE,
            },
        ));
        scene.materials.push(MakeDiffuse(
            light,
            "light",
            ColorRgb::BLACK,
            ColorRgb::New(2.0, 2.0, 2.0),
        ));

        scene.objects.push(SceneObject::New(
            "mirror",
            Geometry::TriangleMesh {
                triangles: vec![
                    Triangle::New(
                        Point3::New(-1.0, -1.0, 0.0),
                        Point3::New(1.0, -1.0, 0.0),
                        Point3::New(1.0, 1.0, 0.0),
                    ),
                    Triangle::New(
                        Point3::New(-1.0, -1.0, 0.0),
                        Point3::New(1.0, 1.0, 0.0),
                        Point3::New(-1.0, 1.0, 0.0),
                    ),
                ],
            },
            mirror,
        ));
        scene.objects.push(SceneObject::New(
            "light",
            Geometry::TriangleMesh {
                triangles: vec![Triangle::New(
                    Point3::New(0.35, -0.35, 0.8),
                    Point3::New(0.85, 0.0, 0.8),
                    Point3::New(0.35, 0.35, 0.8),
                )],
            },
            light,
        ));

        scene
    }

    fn GlassTransmissionScene() -> SceneDescription {
        let camera = Camera::New(
            "main",
            Point3::New(0.0, 0.0, 1.0),
            Vec3::New(0.0, 0.0, -1.0),
            Vec3::Y,
            45.0,
        );
        let glass = MaterialId(0);
        let light = MaterialId(1);

        let mut scene = SceneDescription::New("glass-transmission", camera);
        scene.materials.push(MaterialDescription::New(
            glass,
            "glass",
            MaterialKind::Dielectric {
                refractive_index: 1.5,
            },
        ));
        scene.materials.push(MakeDiffuse(
            light,
            "light",
            ColorRgb::BLACK,
            ColorRgb::New(3.0, 2.5, 2.0),
        ));

        scene.objects.push(SceneObject::New(
            "glass",
            Geometry::TriangleMesh {
                triangles: vec![
                    Triangle::New(
                        Point3::New(-1.0, -1.0, 0.0),
                        Point3::New(1.0, -1.0, 0.0),
                        Point3::New(1.0, 1.0, 0.0),
                    ),
                    Triangle::New(
                        Point3::New(-1.0, -1.0, 0.0),
                        Point3::New(1.0, 1.0, 0.0),
                        Point3::New(-1.0, 1.0, 0.0),
                    ),
                ],
            },
            glass,
        ));
        scene.objects.push(SceneObject::New(
            "light",
            Geometry::TriangleMesh {
                triangles: vec![
                    Triangle::New(
                        Point3::New(-1.0, -1.0, -2.0),
                        Point3::New(1.0, -1.0, -2.0),
                        Point3::New(1.0, 1.0, -2.0),
                    ),
                    Triangle::New(
                        Point3::New(-1.0, -1.0, -2.0),
                        Point3::New(1.0, 1.0, -2.0),
                        Point3::New(-1.0, 1.0, -2.0),
                    ),
                ],
            },
            light,
        ));

        scene
    }

    fn LitRoomScene() -> SceneDescription {
        let camera = Camera::New(
            "main",
            Point3::New(0.0, 0.0, 3.4),
            Vec3::New(0.0, 0.0, -1.0),
            Vec3::Y,
            40.0,
        );

        let red = MaterialId(0);
        let green = MaterialId(1);
        let white = MaterialId(2);
        let light = MaterialId(3);

        let mut scene = SceneDescription::New("lit-room", camera);
        scene.materials.push(MakeDiffuse(
            red,
            "red",
            ColorRgb::New(0.8, 0.2, 0.2),
            ColorRgb::BLACK,
        ));
        scene.materials.push(MakeDiffuse(
            green,
            "green",
            ColorRgb::New(0.2, 0.8, 0.2),
            ColorRgb::BLACK,
        ));
        scene.materials.push(MakeDiffuse(
            white,
            "white",
            ColorRgb::New(0.8, 0.8, 0.8),
            ColorRgb::BLACK,
        ));
        scene.materials.push(MakeDiffuse(
            light,
            "light",
            ColorRgb::BLACK,
            ColorRgb::New(5.0, 4.8, 4.4),
        ));

        scene.objects.push(MakeQuad(
            "floor",
            white,
            Point3::New(-1.2, -1.0, 1.2),
            Point3::New(1.2, -1.0, 1.2),
            Point3::New(1.2, -1.0, -1.2),
            Point3::New(-1.2, -1.0, -1.2),
        ));
        scene.objects.push(MakeQuad(
            "ceiling",
            white,
            Point3::New(-1.2, 1.0, -1.2),
            Point3::New(1.2, 1.0, -1.2),
            Point3::New(1.2, 1.0, 1.2),
            Point3::New(-1.2, 1.0, 1.2),
        ));
        scene.objects.push(MakeQuad(
            "back-wall",
            white,
            Point3::New(-1.2, -1.0, -1.2),
            Point3::New(1.2, -1.0, -1.2),
            Point3::New(1.2, 1.0, -1.2),
            Point3::New(-1.2, 1.0, -1.2),
        ));
        scene.objects.push(MakeQuad(
            "left-wall",
            red,
            Point3::New(-1.2, -1.0, -1.2),
            Point3::New(-1.2, -1.0, 1.2),
            Point3::New(-1.2, 1.0, 1.2),
            Point3::New(-1.2, 1.0, -1.2),
        ));
        scene.objects.push(MakeQuad(
            "right-wall",
            green,
            Point3::New(1.2, -1.0, 1.2),
            Point3::New(1.2, -1.0, -1.2),
            Point3::New(1.2, 1.0, -1.2),
            Point3::New(1.2, 1.0, 1.2),
        ));
        scene.objects.push(MakeQuad(
            "center-panel",
            white,
            Point3::New(-0.45, -1.0, -0.2),
            Point3::New(0.45, -1.0, -0.7),
            Point3::New(0.45, 0.2, -0.7),
            Point3::New(-0.45, 0.2, -0.2),
        ));
        scene.objects.push(MakeQuad(
            "light",
            light,
            Point3::New(-0.35, 0.99, -0.35),
            Point3::New(0.35, 0.99, -0.35),
            Point3::New(0.35, 0.99, 0.35),
            Point3::New(-0.35, 0.99, 0.35),
        ));

        scene
    }

    fn MakeDiffuse(
        material_id: MaterialId,
        name: &str,
        albedo: ColorRgb,
        emission: ColorRgb,
    ) -> MaterialDescription {
        MaterialDescription::New(
            material_id,
            name,
            MaterialKind::Diffuse { albedo, emission },
        )
    }

    fn MakeQuad(
        name: &str,
        material_id: MaterialId,
        a: Point3,
        b: Point3,
        c: Point3,
        d: Point3,
    ) -> SceneObject {
        SceneObject::New(
            name,
            Geometry::TriangleMesh {
                triangles: vec![Triangle::New(a, b, c), Triangle::New(a, c, d)],
            },
            material_id,
        )
    }

    fn AssertColorNear(actual: ColorRgb, expected: ColorRgb, epsilon: f32) {
        assert!((actual.r - expected.r).abs() <= epsilon);
        assert!((actual.g - expected.g).abs() <= epsilon);
        assert!((actual.b - expected.b).abs() <= epsilon);
    }

    fn AssertVec3Near(actual: Vec3, expected: Vec3, epsilon: f32) {
        assert!((actual.x - expected.x).abs() <= epsilon);
        assert!((actual.y - expected.y).abs() <= epsilon);
        assert!((actual.z - expected.z).abs() <= epsilon);
    }
}
