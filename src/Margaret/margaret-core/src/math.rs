use std::ops::{Add, Div, Mul, Neg, Sub};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self::New(0.0, 0.0, 0.0);
    pub const X: Self = Self::New(1.0, 0.0, 0.0);
    pub const Y: Self = Self::New(0.0, 1.0, 0.0);
    pub const Z: Self = Self::New(0.0, 0.0, 1.0);

    pub const fn New(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn Dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn Cross(self, other: Self) -> Self {
        Self::New(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    pub fn LengthSquared(self) -> f32 {
        self.Dot(self)
    }

    pub fn Length(self) -> f32 {
        self.LengthSquared().sqrt()
    }

    pub fn Normalized(self) -> Self {
        let Length = self.Length();
        if Length == 0.0 {
            return Self::ZERO;
        }

        self / Length
    }
}

impl Add for Vec3 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::New(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl Sub for Vec3 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::New(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl Mul<f32> for Vec3 {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        Self::New(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

impl Div<f32> for Vec3 {
    type Output = Self;

    fn div(self, rhs: f32) -> Self::Output {
        Self::New(self.x / rhs, self.y / rhs, self.z / rhs)
    }
}

impl Neg for Vec3 {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self::New(-self.x, -self.y, -self.z)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Point3 {
    pub const ORIGIN: Self = Self::New(0.0, 0.0, 0.0);

    pub const fn New(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

impl Add<Vec3> for Point3 {
    type Output = Self;

    fn add(self, rhs: Vec3) -> Self::Output {
        Self::New(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl Sub<Point3> for Point3 {
    type Output = Vec3;

    fn sub(self, rhs: Point3) -> Self::Output {
        Vec3::New(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl Sub<Vec3> for Point3 {
    type Output = Self;

    fn sub(self, rhs: Vec3) -> Self::Output {
        Self::New(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}
