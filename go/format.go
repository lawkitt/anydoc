//go:build linux && amd64 && cgo

package anydoc

// Format is an explicitly selected product input format.
type Format int

const (
	FormatPDF  Format = 1
	FormatDOCX Format = 2
)
