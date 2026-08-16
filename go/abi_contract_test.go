//go:build linux && amd64 && cgo

package anydoc

import (
	"bufio"
	"errors"
	"os"
	"strconv"
	"strings"
	"testing"
)

func TestOutcomeManifestMatchesGo(t *testing.T) {
	file, err := os.Open("abi/outcomes.tsv")
	if err != nil {
		t.Fatal(err)
	}
	defer file.Close()
	want := map[Outcome]error{
		OutcomeUnsupported:     ErrUnsupported,
		OutcomeMalformed:       ErrInvalid,
		OutcomeEncrypted:       ErrEncrypted,
		OutcomeResourceLimit:   ErrResourceLimit,
		OutcomeMissingPart:     ErrInvalid,
		OutcomeIO:              ErrInternal,
		OutcomeInvalidArgument: ErrInvalidArgument,
		OutcomeScannedOnly:     ErrScannedOnly,
		OutcomePanic:           ErrInternal,
	}
	seen := make(map[Outcome]bool, len(want))
	scanner := bufio.NewScanner(file)
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		fields := strings.Split(line, "\t")
		if len(fields) != 3 {
			t.Fatalf("invalid ABI manifest row: %q", line)
		}
		numeric, err := strconv.Atoi(fields[1])
		if err != nil {
			t.Fatalf("invalid ABI outcome %q: %v", fields[1], err)
		}
		outcome := Outcome(numeric)
		sentinel, ok := want[outcome]
		if !ok {
			t.Fatalf("manifest contains unknown Go outcome: %q", line)
		}
		if !errors.Is(&ConvertError{Outcome: outcome}, sentinel) {
			t.Fatalf("outcome %d does not map to %v", outcome, sentinel)
		}
		seen[outcome] = true
	}
	if err := scanner.Err(); err != nil {
		t.Fatal(err)
	}
	if len(seen) != len(want) {
		t.Fatalf("manifest covers %d outcomes, want %d", len(seen), len(want))
	}
}
