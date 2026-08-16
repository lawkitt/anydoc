//go:build linux && amd64 && cgo

package anydoc

import (
	"bytes"
	"errors"
	"fmt"
	"os"
	"strings"
	"testing"
)

func TestPDFAndDOCXFixtures(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name   string
		path   string
		format Format
	}{
		{name: "pdf", path: "../tests/fixtures/pdf/text.pdf", format: FormatPDF},
		{name: "docx", path: "../tests/fixtures/docx/text.docx", format: FormatDOCX},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()
			input, err := os.ReadFile(test.path)
			if err != nil {
				t.Fatal(err)
			}
			markdown, err := ToMarkdownBytes(input, test.format)
			if err != nil {
				t.Fatal(err)
			}
			if !strings.Contains(string(markdown), "Fixture Document") {
				t.Fatalf("unexpected Markdown: %q", markdown)
			}
		})
	}
}

func TestInvalidArgumentsRemainTyped(t *testing.T) {
	_, err := ToMarkdownBytes(nil, FormatPDF)
	if !errors.Is(err, ErrInvalidArgument) {
		t.Fatalf("empty input error = %v", err)
	}
	_, err = ToMarkdownBytes([]byte("not a document"), Format(100))
	if !errors.Is(err, ErrInvalidArgument) {
		t.Fatalf("unknown format error = %v", err)
	}
}

func TestMalformedPDFRemainsTyped(t *testing.T) {
	_, err := ToMarkdownBytes([]byte("not a PDF"), FormatPDF)
	if !errors.Is(err, ErrInvalid) {
		t.Fatalf("malformed PDF error = %v", err)
	}
}

func TestScannedPDFRemainsTyped(t *testing.T) {
	_, err := ToMarkdownBytes(scannedPDF(), FormatPDF)
	if !errors.Is(err, ErrScannedOnly) {
		t.Fatalf("scanned PDF error = %v", err)
	}
}

func TestInputLimitRemainsTyped(t *testing.T) {
	_, err := ToMarkdownBytes(make([]byte, 20*1024*1024+1), FormatPDF)
	if !errors.Is(err, ErrResourceLimit) {
		t.Fatalf("oversized input error = %v", err)
	}
}

func scannedPDF() []byte {
	objects := []string{
		`<< /Type /Catalog /Pages 2 0 R >>`,
		`<< /Type /Pages /Kids [3 0 R] /Count 1 >>`,
		`<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /XObject << /Im1 5 0 R >> >> /Contents 6 0 R >>`,
		`<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>`,
		"<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length 3 >>\nstream\n\xff\x00\x00\nendstream",
		"<< /Length 31 >>\nstream\nq 612 0 0 792 0 0 cm /Im1 Do Q\nendstream",
	}
	var output bytes.Buffer
	output.WriteString("%PDF-1.7\n%\xff\xff\xff\xff\n")
	offsets := make([]int, 1, len(objects)+1)
	for index, object := range objects {
		offsets = append(offsets, output.Len())
		fmt.Fprintf(&output, "%d 0 obj\n%s\nendobj\n", index+1, object)
	}
	xref := output.Len()
	fmt.Fprintf(&output, "xref\n0 %d\n0000000000 65535 f \n", len(objects)+1)
	for _, offset := range offsets[1:] {
		fmt.Fprintf(&output, "%010d 00000 n \n", offset)
	}
	fmt.Fprintf(
		&output,
		"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n",
		len(objects)+1,
		xref,
	)
	return output.Bytes()
}
