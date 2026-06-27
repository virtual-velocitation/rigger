// Package turbovec is the cgo-backed grounder over the real turbovec Rust engine
// (2-4 bit quantized SIMD vector search) - no Python, no reimplementation.
//
// The implementation is build-tagged `turbovec` (see index.go). Without that tag
// this package is empty and the binary uses the pure-Go grep grounder, so a plain
// `go get` still builds. To use turbovec, build the shim first:
//
//	cd grounder/turbovec/shim && cargo build --release
//
// then build Rigger with `-tags turbovec` (the shim needs a BLAS library, e.g.
// libcblas/libopenblas, on the link path).
package turbovec
