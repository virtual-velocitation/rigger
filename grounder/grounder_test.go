package grounder_test

import (
	"context"
	"os"
	"path/filepath"
	"testing"

	"github.com/virtual-velocitation/rigger/grounder"
)

func TestGrepGroundsMatchingLines(t *testing.T) {
	root := t.TempDir()
	write(t, filepath.Join(root, "damage.go"), "package combat\n\nfunc ApplyDamage(n int) {}\n")
	write(t, filepath.Join(root, "readme.md"), "nothing relevant here\n")

	refs, err := grounder.Grep{Root: root}.Ground(context.Background(), "applydamage", 5)
	if err != nil {
		t.Fatalf("Ground: %v", err)
	}
	if len(refs) != 1 {
		t.Fatalf("want 1 ref, got %d: %v", len(refs), refs)
	}
	if refs[0].File != "damage.go" || refs[0].Line != 3 {
		t.Errorf("ref = %+v, want damage.go:3", refs[0])
	}
}

func TestGrepRespectsK(t *testing.T) {
	root := t.TempDir()
	write(t, filepath.Join(root, "a.go"), "match\nmatch\nmatch\nmatch\n")
	refs, err := grounder.Grep{Root: root}.Ground(context.Background(), "match", 2)
	if err != nil {
		t.Fatal(err)
	}
	if len(refs) != 2 {
		t.Errorf("k=2 should cap results; got %d", len(refs))
	}
}

func TestNopGroundsNothing(t *testing.T) {
	refs, err := grounder.Nop{}.Ground(context.Background(), "anything", 10)
	if err != nil || refs != nil {
		t.Errorf("Nop should ground nothing; got %v, %v", refs, err)
	}
}

func write(t *testing.T, path, content string) {
	t.Helper()
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
}
