# Lawkitt AnyDoc fork

This fork starts from Firecrawl AnyDoc `v0.1.9`, commit
`e754e1d33a1a540ebc9226e36f11d3f401852c9e`. The `upstream` remote is
fetch-only. Lawkitt's native binding design was informed by Firecrawl PR #30,
head commit `1a7a6c04ff8f2689bf8ebbd1e0d9d61d2d109164`, but that older AnyDoc 0.1.6
branch is not part of this fork's lineage.

Fork-owned scope is deliberately narrow:

- a Linux AMD64 glibc Rust/C/Go binding for explicit PDF and DOCX bytes to
  Markdown;
- stable numeric outcomes, scanned-only classification, bounded input and
  Markdown output, and explicit Rust allocation release;
- source-built verification with no committed or downloaded native archives.

The Lawkitt backend owns the helper process, durable worker, cancellation,
resource policy, product error mapping, and deployment. Native conversion must
never be linked into the public API process.

The accepted Rust toolchain is `1.88.0` from
`rust:1.88.0-bookworm@sha256:af306cfa71d987911a781c37b59d7d67d934f49684058f96cf72079c3626bfe0`.
Two clean Linux AMD64 builds with the locked dependency graph and the checkout
mounted at `/src` produced the same static-archive SHA-256:
`35ea6ff798c2b6ac4dce2e2fc202b1260fbe31e2ae693ec477f02d849872b14a`.
