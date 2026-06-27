package worktree_test

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"testing"

	"github.com/virtual-velocitation/rigger/worktree"
)

func TestWorktreeLifecycle(t *testing.T) {
	ctx := context.Background()
	repo := initRepo(t)

	dir := filepath.Join(t.TempDir(), "wt1")
	w, err := worktree.Create(ctx, repo, dir, "unit-1")
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	if w.Branch != "unit-1" {
		t.Errorf("branch = %q, want unit-1", w.Branch)
	}
	if _, err := os.Stat(w.Dir); err != nil {
		t.Fatalf("worktree dir should exist: %v", err)
	}

	// An agent touches a file; ChangedFiles must report it.
	if err := os.WriteFile(filepath.Join(w.Dir, "new.txt"), []byte("hi"), 0o644); err != nil {
		t.Fatal(err)
	}
	changed, err := w.ChangedFiles(ctx)
	if err != nil {
		t.Fatalf("ChangedFiles: %v", err)
	}
	if !slices.Contains(changed, "new.txt") {
		t.Errorf("ChangedFiles = %v, want it to include new.txt", changed)
	}

	if err := w.Remove(ctx); err != nil {
		t.Fatalf("Remove: %v", err)
	}
	if _, err := os.Stat(w.Dir); !os.IsNotExist(err) {
		t.Errorf("worktree dir should be gone after Remove")
	}
}

// initRepo creates a git repository with one commit and returns its path.
func initRepo(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	for _, args := range [][]string{
		{"init", "-q"},
		{"config", "user.email", "t@example.com"},
		{"config", "user.name", "t"},
		{"commit", "--allow-empty", "-q", "-m", "init"},
	} {
		cmd := exec.Command("git", append([]string{"-C", dir}, args...)...)
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v: %s", args, err, out)
		}
	}
	return dir
}
