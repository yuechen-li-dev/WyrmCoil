#![allow(non_snake_case)]

use margaret_core::camera::Camera;
use margaret_core::image::ImageSize;

use super::{CameraRayRequest, CameraRayResult, RayQueryStore, RayVec3};

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
