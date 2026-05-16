#![allow(non_snake_case)]

use dunewyrm::ProjectName;

#[test]
fn SmokeProjectIdentity() {
    assert_eq!(ProjectName(), "Dunewyrm");
}
