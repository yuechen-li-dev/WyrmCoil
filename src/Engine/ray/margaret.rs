#![allow(non_snake_case)]

use margaret_core::camera::Camera;
use margaret_core::image::ImageSize;
use margaret_core::math::{Point3, Vec3};

use super::{
    CameraRayRequest, CameraRayResult, Ray3, RayHitResult, RayMissResult, RayQueryRequest,
    RayQueryRequestStore, RayQueryStore, RayTriangle, RayTriangleScene, RayVec3,
    TriangleRayQueryRequest,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RayQueryExecutionError {
    MissingRequest { QueryId: super::RayQueryId },
}

pub fn ExecuteRayQueryRequestById(
    query_id: super::RayQueryId,
    requests: &mut RayQueryRequestStore,
    results: &mut RayQueryStore,
    camera_ray_adapter: &MargaretCameraRayAdapter,
    mailbox: &mut crate::DwMailbox,
    completion_kind: u32,
) -> Result<(), RayQueryExecutionError> {
    let request = requests
        .Take(query_id)
        .ok_or(RayQueryExecutionError::MissingRequest { QueryId: query_id })?;

    match request {
        RayQueryRequest::CameraRay(camera_request) => {
            camera_ray_adapter.BuildCameraRay(camera_request, results);
        }
        RayQueryRequest::TriangleRay { Request, Scene } => {
            ExecuteTriangleRayQuery(Request, &Scene, results);
        }
    }

    mailbox.Enqueue(crate::DwMessage::I32(completion_kind, query_id.0 as i32));
    Ok(())
}
#[derive(Clone, Debug)]
pub struct MargaretCameraRayAdapter {
    pub Camera: Camera,
    pub Width: u32,
    pub Height: u32,
}

impl MargaretCameraRayAdapter {
    pub fn BuildCameraRay(
        &self,
        request: CameraRayRequest,
        store: &mut RayQueryStore,
    ) -> CameraRayResult {
        assert!((0.0..=1.0).contains(&request.ScreenX));
        assert!((0.0..=1.0).contains(&request.ScreenY));

        let image_size = ImageSize::New(self.Width, self.Height);
        let scaled_x = request.ScreenX * self.Width as f32;
        let scaled_y = request.ScreenY * self.Height as f32;

        let pixel_x = (scaled_x.floor() as u32).min(self.Width.saturating_sub(1));
        let pixel_y = (scaled_y.floor() as u32).min(self.Height.saturating_sub(1));

        let subpixel_x = (scaled_x - pixel_x as f32).clamp(0.0, 1.0);
        let subpixel_y = (scaled_y - pixel_y as f32).clamp(0.0, 1.0);

        let ray = self
            .Camera
            .RayForSubpixel(image_size, pixel_x, pixel_y, subpixel_x, subpixel_y);

        let result = CameraRayResult {
            QueryId: request.QueryId,
            Origin: RayVec3 {
                X: ray.origin.x,
                Y: ray.origin.y,
                Z: ray.origin.z,
            },
            Direction: RayVec3 {
                X: ray.direction.x,
                Y: ray.direction.y,
                Z: ray.direction.z,
            },
        };

        store.StoreCompleted(result);
        result
    }
}

pub fn ExecuteTriangleRayQuery(
    request: TriangleRayQueryRequest,
    scene: &RayTriangleScene,
    store: &mut RayQueryStore,
) {
    let mut best_hit: Option<RayHitResult> = None;
    for triangle in &scene.Triangles {
        if let Some(hit) = IntersectTriangle(request.QueryId, request.Ray, *triangle) {
            match best_hit {
                Some(existing) if existing.Distance <= hit.Distance => {}
                _ => best_hit = Some(hit),
            }
        }
    }

    match best_hit {
        Some(hit) => store.StoreHitResult(hit),
        None => store.StoreMissResult(RayMissResult {
            QueryId: request.QueryId,
        }),
    }
}

fn IntersectTriangle(
    query_id: super::RayQueryId,
    ray: Ray3,
    triangle: RayTriangle,
) -> Option<RayHitResult> {
    let epsilon = 0.000001_f32;
    let origin = ToPoint3(ray.Origin);
    let direction = ToVec3(ray.Direction);
    let a = ToPoint3(triangle.A);
    let b = ToPoint3(triangle.B);
    let c = ToPoint3(triangle.C);

    let edge1 = b - a;
    let edge2 = c - a;
    let pvec = direction.Cross(edge2);
    let determinant = edge1.Dot(pvec);
    if determinant.abs() < epsilon {
        return None;
    }

    let inverse_det = 1.0 / determinant;
    let tvec = origin - a;
    let u = tvec.Dot(pvec) * inverse_det;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }

    let qvec = tvec.Cross(edge1);
    let v = direction.Dot(qvec) * inverse_det;
    if v < 0.0 || (u + v) > 1.0 {
        return None;
    }

    let distance = edge2.Dot(qvec) * inverse_det;
    if distance <= epsilon {
        return None;
    }

    let position = origin + direction * distance;
    let normal = edge1.Cross(edge2).Normalized();
    Some(RayHitResult {
        QueryId: query_id,
        TriangleId: triangle.Id,
        Distance: distance,
        Position: ToRayVec3FromPoint(position),
        Normal: ToRayVec3(normal),
    })
}

fn ToPoint3(value: RayVec3) -> Point3 {
    Point3::New(value.X, value.Y, value.Z)
}
fn ToVec3(value: RayVec3) -> Vec3 {
    Vec3::New(value.X, value.Y, value.Z)
}
fn ToRayVec3(value: Vec3) -> RayVec3 {
    RayVec3 {
        X: value.x,
        Y: value.y,
        Z: value.z,
    }
}
fn ToRayVec3FromPoint(value: Point3) -> RayVec3 {
    RayVec3 {
        X: value.x,
        Y: value.y,
        Z: value.z,
    }
}

#[cfg(test)]
mod tests {
    use margaret_core::math::{Point3, Vec3};

    use super::*;
    use crate::Engine::ray::{RayQueryId, RayQueryStore};

    fn Approx(a: f32, b: f32) {
        let delta = (a - b).abs();
        assert!(delta < 0.0001, "a={a} b={b} delta={delta}");
    }

    #[test]
    fn CenterRayFacesCameraForward() {
        let camera = Camera::New("main", Point3::ORIGIN, -Vec3::Z, Vec3::Y, 90.0);
        let adapter = MargaretCameraRayAdapter {
            Camera: camera,
            Width: 100,
            Height: 100,
        };
        let mut store = RayQueryStore::New();

        let result = adapter.BuildCameraRay(
            CameraRayRequest {
                QueryId: RayQueryId(4),
                ScreenX: 0.5,
                ScreenY: 0.5,
            },
            &mut store,
        );

        Approx(result.Origin.X, 0.0);
        Approx(result.Origin.Y, 0.0);
        Approx(result.Origin.Z, 0.0);
        Approx(result.Direction.X, 0.0);
        Approx(result.Direction.Y, 0.0);
        Approx(result.Direction.Z, -1.0);
        Approx(
            (result.Direction.X * result.Direction.X
                + result.Direction.Y * result.Direction.Y
                + result.Direction.Z * result.Direction.Z)
                .sqrt(),
            1.0,
        );
    }

    #[test]
    fn TopLeftRayShowsYDownViewportMapping() {
        let camera = Camera::New("main", Point3::ORIGIN, -Vec3::Z, Vec3::Y, 90.0);
        let adapter = MargaretCameraRayAdapter {
            Camera: camera,
            Width: 100,
            Height: 100,
        };
        let mut store = RayQueryStore::New();

        let result = adapter.BuildCameraRay(
            CameraRayRequest {
                QueryId: RayQueryId(9),
                ScreenX: 0.0,
                ScreenY: 0.0,
            },
            &mut store,
        );

        assert!(result.Direction.X < 0.0);
        assert!(result.Direction.Y > 0.0);
        assert!(result.Direction.Z < 0.0);
    }
}
