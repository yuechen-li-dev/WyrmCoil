#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

pub const ProjectName: fn() -> &'static str = || "WyrmCoil";

pub mod Dunewyrm {
    pub const ModuleName: &str = "Dunewyrm";
}

pub mod Engine {
    pub const ModuleName: &str = "WyrmCoilEngine";
}

#[cfg(test)]
mod tests {
    use super::ProjectName;

    #[test]
    fn SmokeProjectIdentity() {
        assert_eq!(ProjectName(), "WyrmCoil");
    }
}
