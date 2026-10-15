//! The migrating-from-plotters guide cannot drift from its gallery images:
//! every runnable snippet in `docs/migrating-from-plotters.md` (compiled
//! and run as a doc test through `rizzma::guide`) must appear verbatim in
//! the gallery example between `// migrate: <name>` and `// end migrate`,
//! and the guide must show that snippet's `gallery_migrate_<name>.png`.

const GUIDE: &str = include_str!("../docs/migrating-from-plotters.md");
const GALLERY: &str = include_str!("../examples/gallery.rs");

/// The guide's runnable snippets: `(name, visible lines)`, named by their
/// hidden `# // migrate: <name>` line. Hidden (`# `) lines are rustdoc
/// setup and are not compared.
fn guide_snippets() -> Vec<(String, Vec<String>)> {
    let mut snippets = Vec::new();
    let mut block: Option<Vec<&str>> = None;
    for line in GUIDE.lines() {
        match &mut block {
            None if line == "```rust" => block = Some(Vec::new()),
            None => {}
            Some(lines) if line == "```" => {
                let name = lines
                    .iter()
                    .find_map(|l| l.strip_prefix("# // migrate: "))
                    .unwrap_or_else(|| {
                        panic!("a runnable snippet without a migrate name: {lines:?}")
                    })
                    .to_owned();
                let visible = lines
                    .iter()
                    .filter(|l| !(l.starts_with("# ") || **l == "#"))
                    .map(|l| (*l).to_owned())
                    .collect();
                snippets.push((name, visible));
                block = None;
            }
            Some(lines) => lines.push(line),
        }
    }
    snippets
}

/// The gallery's copy of snippet `name`, dedented to its marker.
fn gallery_snippet(name: &str) -> Option<Vec<String>> {
    let marker = format!("// migrate: {name}");
    let mut lines = GALLERY.lines();
    let start = lines.by_ref().find(|l| l.trim() == marker)?;
    let indent = start.len() - start.trim_start().len();
    Some(
        lines
            .take_while(|l| l.trim() != "// end migrate")
            .map(|l| l.get(indent..).unwrap_or("").to_owned())
            .collect(),
    )
}

#[test]
fn every_guide_snippet_is_drawn_by_the_gallery() {
    let snippets = guide_snippets();
    assert_eq!(snippets.len(), 11, "ten idioms and the worked port");
    for (name, guide) in &snippets {
        let gallery = gallery_snippet(name)
            .unwrap_or_else(|| panic!("no `// migrate: {name}` block in examples/gallery.rs"));
        for (i, (g, d)) in guide.iter().zip(&gallery).enumerate() {
            assert_eq!(
                g,
                d,
                "snippet `{name}` line {} differs from the gallery's",
                i + 1
            );
        }
        assert_eq!(
            guide.len(),
            gallery.len(),
            "snippet `{name}` length differs from the gallery's"
        );
        assert!(
            GUIDE.contains(&format!("gallery_migrate_{name}.png")),
            "the guide does not show gallery_migrate_{name}.png"
        );
        assert!(
            GALLERY.contains(&format!("\"target/gallery_migrate_{name}.png\"")),
            "the gallery does not write gallery_migrate_{name}.png"
        );
    }
}

#[test]
fn every_gallery_migrate_block_is_in_the_guide() {
    let names: Vec<String> = guide_snippets().into_iter().map(|(n, _)| n).collect();
    for line in GALLERY.lines() {
        if let Some(name) = line.trim().strip_prefix("// migrate: ") {
            assert!(
                names.iter().any(|n| n == name),
                "gallery block `{name}` is not in the guide"
            );
        }
    }
}
