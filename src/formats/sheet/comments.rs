//! XLSX comments, counted only: each visible sheet's threaded comments
//! (replies included) and its notes (legacy comments), a note that is a
//! thread's legacy copy (same cell) not counted twice. A comment part that is
//! missing, malformed or over a limit is skipped, never failing the
//! conversion.

use crate::package::Package;
use crate::package::path;
use crate::package::relationships::{Relationship, TargetMode, read_rels, rels_part_for};
use crate::package::xml::{Element, ns};
use crate::shared::comments::read_part;
use std::collections::HashSet;

const NOTES_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";
const THREADED_REL: &str =
    "http://schemas.microsoft.com/office/2017/10/relationships/threadedComment";
const THREADED: &str = "http://schemas.microsoft.com/office/spreadsheetml/2018/threadedcomments";

/// Comments on the given sheets (`(name, part)` pairs).
pub(super) fn count(pkg: &mut Package, sheets: &[(String, String)]) -> u32 {
    let mut found = 0usize;
    for (_, sheet) in sheets {
        let rels = match read_rels(pkg, &rels_part_for(sheet)) {
            Ok(rels) => rels,
            Err(e) => {
                log::warn!("skipping comments of {sheet}: {e}");
                continue;
            }
        };
        let load = |pkg: &mut Package, rel: &Relationship| -> Option<Element> {
            if rel.mode != TargetMode::Internal {
                return None;
            }
            read_part(pkg, &path::resolve(sheet, &rel.target).ok()?.path)
        };
        let mut threaded_cells = HashSet::new();
        for (_, rel) in rels.iter().filter(|(_, r)| r.rel_type == THREADED_REL) {
            let Some(tree) = load(pkg, rel) else { continue };
            for comment in tree.descendants(THREADED, "threadedComment") {
                found += 1;
                if let Some(cell) = comment.attr_unqualified("ref") {
                    threaded_cells.insert(cell.to_ascii_uppercase());
                }
            }
        }
        for (_, rel) in rels.iter().filter(|(_, r)| r.rel_type == NOTES_REL) {
            let Some(tree) = load(pkg, rel) else { continue };
            found += tree
                .descendants(ns::SML, "comment")
                .filter(|c| {
                    c.attr_unqualified("ref")
                        .is_none_or(|cell| !threaded_cells.contains(&cell.to_ascii_uppercase()))
                })
                .count();
        }
    }
    u32::try_from(found).unwrap_or(u32::MAX)
}
