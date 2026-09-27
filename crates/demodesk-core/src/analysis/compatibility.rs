//! Proven compatibility evidence for version-dependent animation and shot settings.
//! Unknown versions stay unqualified until their behavior is checked against a game capture.

pub const CS2_WRITE_SET_CLIENT_SHA256: &str =
    "a0c195f0b6ec00915ef08c548200a010ebbe7982d3a4bc468cad939b67c8c4e3";
pub const CS2_WRITE_SET_CLIENT_SHA256_V3: &str =
    "9b4f46dbd6a433163b39d7ea0123c321b1ad6d95ceedd40ae121312464833549";

pub fn custom_animation_tasks(client_sha256: &str) -> bool {
    [CS2_WRITE_SET_CLIENT_SHA256, CS2_WRITE_SET_CLIENT_SHA256_V3].contains(&client_sha256)
}

pub fn spread_defaults(patch: Option<i32>, demo_version: Option<&str>) -> bool {
    matches!(patch, Some(14181 | 14185)) && demo_version == Some("valve_demo_2")
}

pub fn rewind_defaults(patch: Option<i32>) -> bool {
    patch == Some(14181)
}

#[cfg(test)]
mod tests {
    #[test]
    fn known_capabilities_do_not_qualify_unseen_versions() {
        use super::*;
        assert!(custom_animation_tasks(CS2_WRITE_SET_CLIENT_SHA256_V3));
        assert!(!custom_animation_tasks("updated-client"));
        assert!(spread_defaults(Some(14185), Some("valve_demo_2")));
        assert!(!spread_defaults(Some(14186), Some("valve_demo_2")));
        assert!(!rewind_defaults(Some(14185)));
    }
}
