//! PPTX comments, counted only: the legacy (`p:cm`) and modern
//! (`p188:cm`, `p188:reply`) comment parts each slide relates to. A comment
//! part that is missing, malformed or over a limit is skipped, never failing
//! the conversion.

use crate::package::Package;
use crate::package::path;
use crate::package::relationships::{Relationships, TargetMode};
use crate::package::xml::ns;
use crate::shared::comments::read_part;
use std::cell::RefCell;
use std::collections::HashSet;

const LEGACY_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";
const MODERN_REL: &str = "http://schemas.microsoft.com/office/2018/10/relationships/comments";
const P188: &str = "http://schemas.microsoft.com/office/powerpoint/2018/8/main";

/// Comments and replies on the slides, each comment part counted once.
pub(super) fn count(pkg: &RefCell<Package>, slides: &[String], rels: &[Relationships]) -> u32 {
    let mut parts = HashSet::new();
    let mut found = 0usize;
    for (slide, rels) in slides.iter().zip(rels) {
        for (_, rel) in rels.iter() {
            let modern = match rel.rel_type.as_str() {
                LEGACY_REL => false,
                MODERN_REL => true,
                _ => continue,
            };
            if rel.mode != TargetMode::Internal {
                continue;
            }
            let Ok(target) = path::resolve(slide, &rel.target) else {
                continue;
            };
            if !parts.insert(target.path.clone()) {
                continue;
            }
            let Some(tree) = read_part(&mut pkg.borrow_mut(), &target.path) else {
                continue;
            };
            found += tree
                .descendant_elems()
                .filter(|e| match modern {
                    true => e.is(P188, "cm") || e.is(P188, "reply"),
                    false => e.is(ns::P, "cm"),
                })
                .count();
        }
    }
    u32::try_from(found).unwrap_or(u32::MAX)
}
