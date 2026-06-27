//go:build turbovec

package turbovec

/*
#cgo LDFLAGS: ${SRCDIR}/shim/target/release/libriggervec.a -l:libcblas.so.3 -lm -ldl -lpthread
#include "index.h"
*/
import "C"

import (
	"fmt"
	"runtime"
	"unsafe"
)

// Index is a turbovec vector index over dim-dimensional vectors.
type Index struct {
	ptr *C.IdMapIndex
	dim int
}

// NewIndex creates an index over dim-dimensional vectors, quantized to bitWidth
// (2, 3, or 4) bits per dimension.
func NewIndex(dim, bitWidth int) (*Index, error) {
	ptr := C.rv_index_new(C.size_t(dim), C.size_t(bitWidth))
	if ptr == nil {
		return nil, fmt.Errorf("turbovec: new index (dim=%d, bits=%d) failed", dim, bitWidth)
	}
	idx := &Index{ptr: ptr, dim: dim}
	runtime.SetFinalizer(idx, (*Index).Free)
	return idx, nil
}

// Add inserts n vectors (row-major, len == n*dim) with their ids (len == n).
func (i *Index) Add(vectors []float32, ids []uint64) error {
	if len(ids) == 0 {
		return nil
	}
	rc := C.rv_index_add(
		i.ptr,
		(*C.float)(unsafe.Pointer(&vectors[0])),
		C.size_t(len(ids)),
		C.size_t(i.dim),
		(*C.uint64_t)(unsafe.Pointer(&ids[0])),
	)
	if rc != 0 {
		return fmt.Errorf("turbovec: add %d vectors failed", len(ids))
	}
	return nil
}

// Search returns the ids and scores of the up-to-k nearest vectors to query.
func (i *Index) Search(query []float32, k int) (ids []uint64, scores []float32) {
	if k <= 0 || len(query) != i.dim {
		return nil, nil
	}
	ids = make([]uint64, k)
	scores = make([]float32, k)
	n := int(C.rv_index_search(
		i.ptr,
		(*C.float)(unsafe.Pointer(&query[0])),
		C.size_t(i.dim),
		C.size_t(k),
		(*C.uint64_t)(unsafe.Pointer(&ids[0])),
		(*C.float)(unsafe.Pointer(&scores[0])),
	))
	if n < 0 {
		n = 0
	}
	return ids[:n], scores[:n]
}

// Free releases the index. Safe to call more than once.
func (i *Index) Free() {
	if i.ptr != nil {
		C.rv_index_free(i.ptr)
		i.ptr = nil
	}
}
