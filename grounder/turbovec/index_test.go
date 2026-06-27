//go:build turbovec

package turbovec

import "testing"

func TestIndexAddAndSearch(t *testing.T) {
	idx, err := NewIndex(8, 4) // dim must be a positive multiple of 8
	if err != nil {
		t.Fatal(err)
	}
	defer idx.Free()

	// Three normalized basis vectors; the query equals e2, so id 2 must win.
	vecs := []float32{
		1, 0, 0, 0, 0, 0, 0, 0, // id 0
		0, 1, 0, 0, 0, 0, 0, 0, // id 1
		0, 0, 1, 0, 0, 0, 0, 0, // id 2
	}
	if err := idx.Add(vecs, []uint64{0, 1, 2}); err != nil {
		t.Fatal(err)
	}
	ids, _ := idx.Search([]float32{0, 0, 1, 0, 0, 0, 0, 0}, 1)
	if len(ids) != 1 || ids[0] != 2 {
		t.Errorf("expected id 2 nearest the query, got %v", ids)
	}
}
