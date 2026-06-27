// Package worktree isolates a unit of work in a throwaway git worktree branched
// from HEAD, so parallel units cannot conflict on the filesystem (architecture
// R5): isolation guards the files, while the event stream is the shared decision
// channel. It also reports which files an agent touched, which feeds FileTouched
// events into the context graph.
package worktree

import (
	"context"
	"fmt"
	"os/exec"
	"strings"
)

// Worktree is an isolated git worktree for one unit of work.
type Worktree struct {
	Dir    string // the worktree's working directory
	Branch string // the branch it is on
	repo   string // the parent repository
}

// Create adds a worktree at dir (which must not already exist), on a new branch
// off the repo's current HEAD.
func Create(ctx context.Context, repo, dir, branch string) (*Worktree, error) {
	if out, err := git(ctx, repo, "worktree", "add", "-b", branch, dir, "HEAD"); err != nil {
		return nil, fmt.Errorf("worktree: add %s: %w: %s", dir, err, out)
	}
	return &Worktree{Dir: dir, Branch: branch, repo: repo}, nil
}

// ChangedFiles returns the paths an agent created or modified in the worktree
// (staged, unstaged, or untracked).
func (w *Worktree) ChangedFiles(ctx context.Context) ([]string, error) {
	out, err := git(ctx, w.Dir, "status", "--porcelain")
	if err != nil {
		return nil, fmt.Errorf("worktree: status: %w: %s", err, out)
	}
	var files []string
	for _, line := range strings.Split(out, "\n") {
		// porcelain lines are "XY <path>"; the path starts at column 3.
		if len(line) > 3 {
			files = append(files, strings.TrimSpace(line[3:]))
		}
	}
	return files, nil
}

// Integrate commits the agent's changes on the worktree's branch and merges that
// branch into the repository's base branch, returning the new commit hash. A
// read-only stage (no changes) commits nothing and returns "".
func (w *Worktree) Integrate(ctx context.Context, message string) (string, error) {
	if out, err := git(ctx, w.Dir, "add", "-A"); err != nil {
		return "", fmt.Errorf("worktree: add: %w: %s", err, out)
	}
	out, err := git(ctx, w.Dir, "commit", "-m", message)
	if err != nil {
		if strings.Contains(out, "nothing to commit") {
			return "", nil // a read-only stage changed nothing
		}
		return "", fmt.Errorf("worktree: commit: %w: %s", err, out)
	}
	head, err := git(ctx, w.Dir, "rev-parse", "HEAD")
	if err != nil {
		return "", fmt.Errorf("worktree: rev-parse: %w: %s", err, head)
	}
	commit := strings.TrimSpace(head)
	if out, err := git(ctx, w.repo, "merge", "--no-edit", w.Branch); err != nil {
		return commit, fmt.Errorf("worktree: merge %s into base: %w: %s", w.Branch, err, out)
	}
	return commit, nil
}

// Remove deletes the worktree (its branch is left for the caller to clean up or
// merge).
func (w *Worktree) Remove(ctx context.Context) error {
	if out, err := git(ctx, w.repo, "worktree", "remove", "--force", w.Dir); err != nil {
		return fmt.Errorf("worktree: remove %s: %w: %s", w.Dir, err, out)
	}
	return nil
}

func git(ctx context.Context, dir string, args ...string) (string, error) {
	out, err := exec.CommandContext(ctx, "git", append([]string{"-C", dir}, args...)...).CombinedOutput()
	return string(out), err
}
