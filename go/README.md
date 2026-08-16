# Lawkitt Go binding

This directory is the minimal native binding used by Lawkitt's isolated
document helper. It supports only explicit PDF and DOCX byte conversion to
Markdown. It deliberately does not expose AnyDoc's document model, assets,
path helpers, format detection, error strings, or prebuilt platform archives.

The ABI is Linux AMD64 glibc only. Build the Rust archive from this checkout
before building or testing Go:

```sh
cargo build --locked --release -p anydoc-go
CGO_ENABLED=1 CGO_LDFLAGS="-L$PWD/target/release" go test ./go/...
```

Rust owns successful output buffers; Go copies and frees them in the same cgo
call sequence. Conversion is synchronous and not cancellable in-process. The
Lawkitt backend therefore invokes its cgo-linked helper as a short-lived child
process and owns deadlines, resource limits, and crash containment there.
