use margaret_core::camera::Camera;
use margaret_core::color::ColorRgb;
use margaret_core::image::ImageSize;
use margaret_core::material::{MaterialDescription, MaterialId, MaterialKind};
use margaret_core::math::{Point3, Vec3};
use margaret_core::scene::{Geometry, SceneDescription, SceneObject, Triangle};

pub fn SampleImageSize() -> ImageSize {
    ImageSize::New(640, 360)
}

pub fn SampleScene() -> SceneDescription {
    let camera = Camera::New(
        "main",
        Point3::New(0.0, 0.0, 3.0),
        Vec3::New(0.0, 0.0, -1.0),
        Vec3::Y,
        45.0,
    );
    let material_id = MaterialId(0);

    let mut scene = SceneDescription::New("triangle-scene", camera);
    scene.materials.push(MaterialDescription::New(
        material_id,
        "matte-gray",
        MaterialKind::Diffuse {
            albedo: ColorRgb::New(0.5, 0.5, 0.5),
            emission: ColorRgb::BLACK,
        },
    ));
    scene.objects.push(SceneObject::New(
        "preview-triangles",
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
