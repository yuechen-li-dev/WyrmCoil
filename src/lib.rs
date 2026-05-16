#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

pub const ProjectName: fn() -> &'static str = || "WyrmCoil";

pub mod Dunewyrm;
pub mod Engine;
pub mod wyrmfmt;

pub use Dunewyrm::*;

#[cfg(test)]
mod tests {
    use super::ProjectName;

    #[test]
    fn SmokeProjectIdentity() {
        assert_eq!(ProjectName(), "WyrmCoil");
    }
}
