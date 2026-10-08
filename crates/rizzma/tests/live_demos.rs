//! The crate docs and the demo site share one set of live demos
//! (`www/rizzma-demos.js`): the docs mount the cells their `data-demo`
//! attributes name, the demo site mounts every entry. This keeps the docs from
//! naming a demo the shared module does not define — which would silently
//! leave that cell a static image and break the demo site's superset.

const CRATE_DOCS: &str = include_str!("../src/lib.rs");
const DEMOS: &str = include_str!("../www/rizzma-demos.js");

/// Every `data-demo="…"` name in the crate-root docs.
fn docs_demo_names() -> Vec<&'static str> {
    CRATE_DOCS
        .split("data-demo=\"")
        .skip(1)
        .map(|rest| &rest[..rest.find('"').expect("unterminated data-demo")])
        .collect()
}

/// Every builder defined in the shared module's `demos` registry.
fn shared_demo_names() -> Vec<&'static str> {
    let registry = &DEMOS[DEMOS
        .find("export const demos = {")
        .expect("rizzma-demos.js exports `demos`")..];
    registry
        .lines()
        .filter_map(|line| line.strip_prefix("  ")?.split_once("(mod, canvas, host"))
        .map(|(name, _)| name)
        .filter(|name| !name.starts_with(' '))
        .collect()
}

#[test]
fn docs_cells_name_shared_demos() {
    let docs = docs_demo_names();
    let shared = shared_demo_names();
    assert!(!docs.is_empty(), "no data-demo cells found in lib.rs");
    for name in &docs {
        assert!(
            shared.contains(name),
            "lib.rs mounts data-demo=\"{name}\" but www/rizzma-demos.js defines \
             no such demo (defined: {shared:?})"
        );
    }
}

#[test]
fn shared_demo_names_are_unique() {
    let shared = shared_demo_names();
    let mut sorted = shared.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), shared.len(), "duplicate demo in {shared:?}");
}
