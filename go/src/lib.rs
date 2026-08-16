//! Minimal C ABI used by Lawkitt's process-contained Go helper.
//!
//! The ABI deliberately exposes only PDF/DOCX to Markdown conversion. Rust
//! allocates successful output and the caller releases it with
//! [`lawkitt_anydoc_buffer_free`]. Failures are stable numeric outcomes; human
//! diagnostics are not protocol and never require a second thread-local call.

#![allow(clippy::missing_safety_doc)]

use std::ffi::c_int;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

use anydoc::{ConvertError, Format};

const MAX_INPUT_BYTES: usize = 20 * 1024 * 1024;
const MAX_MARKDOWN_BYTES: usize = 10 * 1024 * 1024;

pub const OUTCOME_OK: c_int = 0;
pub const OUTCOME_UNSUPPORTED: c_int = 2;
pub const OUTCOME_MALFORMED: c_int = 3;
pub const OUTCOME_ENCRYPTED: c_int = 4;
pub const OUTCOME_RESOURCE_LIMIT: c_int = 5;
pub const OUTCOME_MISSING_PART: c_int = 6;
pub const OUTCOME_IO: c_int = 7;
pub const OUTCOME_INVALID_ARGUMENT: c_int = 8;
pub const OUTCOME_SCANNED_ONLY: c_int = 9;
pub const OUTCOME_PANIC: c_int = 10;

pub const FORMAT_PDF: c_int = 1;
pub const FORMAT_DOCX: c_int = 2;

fn map_error(error: &ConvertError) -> c_int {
    match error {
        ConvertError::Unsupported(_) => OUTCOME_UNSUPPORTED,
        ConvertError::Malformed { .. } => OUTCOME_MALFORMED,
        ConvertError::Encrypted => OUTCOME_ENCRYPTED,
        ConvertError::ResourceLimit { .. } => OUTCOME_RESOURCE_LIMIT,
        ConvertError::MissingPart { .. } => OUTCOME_MISSING_PART,
        ConvertError::Io(_) => OUTCOME_IO,
        _ => OUTCOME_IO,
    }
}

fn is_scanned_only(bytes: &[u8], format: Format) -> bool {
    if format != Format::Pdf {
        return false;
    }
    match pdf_inspector::detect_pdf_type_mem(bytes) {
        Ok(result) => matches!(
            result.pdf_type,
            pdf_inspector::PdfType::Scanned | pdf_inspector::PdfType::ImageBased
        ),
        // AnyDoc owns malformed/encrypted classification when inspection fails.
        Err(_) => false,
    }
}

fn convert(bytes: &[u8], format_tag: c_int) -> Result<Box<[u8]>, c_int> {
    if bytes.is_empty() || bytes.len() > MAX_INPUT_BYTES {
        return Err(if bytes.is_empty() {
            OUTCOME_INVALID_ARGUMENT
        } else {
            OUTCOME_RESOURCE_LIMIT
        });
    }
    let format = match format_tag {
        FORMAT_PDF => Format::Pdf,
        FORMAT_DOCX => Format::Docx,
        _ => return Err(OUTCOME_INVALID_ARGUMENT),
    };
    if is_scanned_only(bytes, format) {
        return Err(OUTCOME_SCANNED_ONLY);
    }
    let markdown = anydoc::to_markdown_bytes(bytes, format).map_err(|error| map_error(&error))?;
    if markdown.len() > MAX_MARKDOWN_BYTES {
        return Err(OUTCOME_RESOURCE_LIMIT);
    }
    Ok(markdown.into_bytes().into_boxed_slice())
}

fn guarded_convert(
    operation: impl FnOnce() -> Result<Box<[u8]>, c_int>,
) -> Result<Box<[u8]>, c_int> {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(result) => result,
        Err(_) => Err(OUTCOME_PANIC),
    }
}

/// Convert one PDF or DOCX byte slice to Markdown.
///
/// `out_ptr` and `out_len` are zeroed before conversion. On success the caller
/// owns the returned allocation and must pass it, with the exact length, to
/// [`lawkitt_anydoc_buffer_free`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lawkitt_anydoc_to_markdown(
    bytes: *const u8,
    len: usize,
    format_tag: c_int,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> c_int {
    if out_ptr.is_null() || out_len.is_null() {
        return OUTCOME_INVALID_ARGUMENT;
    }
    unsafe {
        *out_ptr = ptr::null_mut();
        *out_len = 0;
    }
    if bytes.is_null() || len == 0 || len > MAX_INPUT_BYTES {
        return if len > MAX_INPUT_BYTES {
            OUTCOME_RESOURCE_LIMIT
        } else {
            OUTCOME_INVALID_ARGUMENT
        };
    }
    let input = unsafe { std::slice::from_raw_parts(bytes, len) };
    match guarded_convert(|| convert(input, format_tag)) {
        Ok(markdown) => {
            let len = markdown.len();
            if len == 0 {
                return OUTCOME_OK;
            }
            let raw = Box::into_raw(markdown) as *mut u8;
            unsafe {
                *out_ptr = raw;
                *out_len = len;
            }
            OUTCOME_OK
        }
        Err(outcome) => outcome,
    }
}

/// Release a buffer returned by [`lawkitt_anydoc_to_markdown`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lawkitt_anydoc_buffer_free(buffer: *mut u8, len: usize) {
    if buffer.is_null() || len == 0 {
        return;
    }
    let slice = ptr::slice_from_raw_parts_mut(buffer, len);
    unsafe { drop(Box::from_raw(slice)) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_values_are_stable() {
        let outcomes = [
            ("OUTCOME_UNSUPPORTED", OUTCOME_UNSUPPORTED),
            ("OUTCOME_MALFORMED", OUTCOME_MALFORMED),
            ("OUTCOME_ENCRYPTED", OUTCOME_ENCRYPTED),
            ("OUTCOME_RESOURCE_LIMIT", OUTCOME_RESOURCE_LIMIT),
            ("OUTCOME_MISSING_PART", OUTCOME_MISSING_PART),
            ("OUTCOME_IO", OUTCOME_IO),
            ("OUTCOME_INVALID_ARGUMENT", OUTCOME_INVALID_ARGUMENT),
            ("OUTCOME_SCANNED_ONLY", OUTCOME_SCANNED_ONLY),
            ("OUTCOME_PANIC", OUTCOME_PANIC),
        ];
        let manifest = include_str!("../abi/outcomes.tsv");
        let header = include_str!("../include/anydoc.h");
        let mut seen = [false; 9];
        for line in manifest.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fields: Vec<_> = line.split('\t').collect();
            assert_eq!(fields.len(), 3, "invalid ABI manifest row: {line}");
            let (index, (_, outcome)) = outcomes
                .iter()
                .enumerate()
                .find(|(_, (name, _))| *name == fields[0])
                .unwrap_or_else(|| panic!("manifest names unknown Rust outcome: {line}"));
            seen[index] = true;
            assert_eq!(outcome.to_string(), fields[1], "numeric outcome drift: {line}");
            assert!(
                header.contains(&format!("#define LAWKITT_ANYDOC_{} {}", fields[0], fields[1])),
                "C header drift: {line}"
            );
        }
        assert!(seen.iter().all(|value| *value), "manifest omits a Rust outcome");
        assert_eq!(OUTCOME_OK, 0);
        assert!(header.contains("#define LAWKITT_ANYDOC_OUTCOME_OK 0"));
    }

    #[test]
    fn invalid_input_is_rejected_without_an_allocation() {
        let mut output = ptr::dangling_mut::<u8>();
        let mut length = usize::MAX;
        let outcome = unsafe {
            lawkitt_anydoc_to_markdown(ptr::null(), 0, FORMAT_PDF, &mut output, &mut length)
        };
        assert_eq!(outcome, OUTCOME_INVALID_ARGUMENT);
        assert!(output.is_null());
        assert_eq!(length, 0);
    }

    #[test]
    fn panic_is_contained_as_a_numeric_outcome() {
        let outcome = guarded_convert(|| -> Result<Box<[u8]>, c_int> { panic!("fixture panic") });
        assert_eq!(outcome.unwrap_err(), OUTCOME_PANIC);
    }
}
