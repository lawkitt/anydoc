//go:build linux && amd64 && cgo

package anydoc

import (
	"errors"
	"fmt"
)

var (
	ErrUnsupported     = errors.New("anydoc: unsupported document")
	ErrInvalid         = errors.New("anydoc: invalid document")
	ErrEncrypted       = errors.New("anydoc: encrypted document")
	ErrResourceLimit   = errors.New("anydoc: resource limit")
	ErrScannedOnly     = errors.New("anydoc: scanned-only document")
	ErrInvalidArgument = errors.New("anydoc: invalid binding argument")
	ErrInternal        = errors.New("anydoc: internal conversion failure")
)

// Outcome is the stable numeric result returned by the Rust ABI.
type Outcome int

const (
	OutcomeOK              Outcome = 0
	OutcomeUnsupported     Outcome = 2
	OutcomeMalformed       Outcome = 3
	OutcomeEncrypted       Outcome = 4
	OutcomeResourceLimit   Outcome = 5
	OutcomeMissingPart     Outcome = 6
	OutcomeIO              Outcome = 7
	OutcomeInvalidArgument Outcome = 8
	OutcomeScannedOnly     Outcome = 9
	OutcomePanic           Outcome = 10
)

// ConvertError carries only the stable outcome. Rust diagnostic strings are
// intentionally not part of the binding protocol.
type ConvertError struct {
	Outcome Outcome
}

func (e *ConvertError) Error() string {
	return fmt.Sprintf("%s (outcome %d)", e.Unwrap(), e.Outcome)
}

func (e *ConvertError) Unwrap() error {
	switch e.Outcome {
	case OutcomeUnsupported:
		return ErrUnsupported
	case OutcomeMalformed, OutcomeMissingPart:
		return ErrInvalid
	case OutcomeEncrypted:
		return ErrEncrypted
	case OutcomeResourceLimit:
		return ErrResourceLimit
	case OutcomeScannedOnly:
		return ErrScannedOnly
	case OutcomeInvalidArgument:
		return ErrInvalidArgument
	default:
		return ErrInternal
	}
}
