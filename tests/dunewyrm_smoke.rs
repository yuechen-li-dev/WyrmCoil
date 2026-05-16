#![allow(non_snake_case)]

use wyrmcoil::Dunewyrm;

#[test]
fn SmokeProjectIdentity() {
    assert_eq!(Dunewyrm::ProjectName(), "Dunewyrm");
}
