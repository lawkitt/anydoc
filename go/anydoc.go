//go:build linux && amd64 && cgo

// Package anydoc exposes Lawkitt's minimal native PDF/DOCX Markdown binding.
package anydoc

/*
#cgo CFLAGS: -I${SRCDIR}/include
#include <stdint.h>
#include "include/anydoc.h"
*/
import "C"

import (
	"runtime"
	"unsafe"
)

const (
	Version    = "0.1.9"
	ABIVersion = 1
)

// ToMarkdownBytes converts one explicitly typed document. The Rust call is
// synchronous; callers that need cancellation must execute it behind an OS
// process boundary.
func ToMarkdownBytes(data []byte, format Format) ([]byte, error) {
	if len(data) == 0 {
		return nil, &ConvertError{Outcome: OutcomeInvalidArgument}
	}
	if format != FormatPDF && format != FormatDOCX {
		return nil, &ConvertError{Outcome: OutcomeInvalidArgument}
	}
	var output *C.uint8_t
	var outputLength C.size_t
	outcome := Outcome(C.lawkitt_anydoc_to_markdown(
		(*C.uint8_t)(unsafe.Pointer(&data[0])),
		C.size_t(len(data)),
		C.int(format),
		&output,
		&outputLength,
	))
	runtime.KeepAlive(data)
	if outcome != OutcomeOK {
		return nil, &ConvertError{Outcome: outcome}
	}
	if output == nil || outputLength == 0 {
		return []byte{}, nil
	}
	defer C.lawkitt_anydoc_buffer_free(output, outputLength)
	return C.GoBytes(unsafe.Pointer(output), C.int(outputLength)), nil
}
