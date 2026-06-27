// Package grounder is the pluggable grounding port: given a query, it returns the
// locations an agent should read before working (architecture §5.4). The default
// is grep (literal search); a vector impl (semantic) plugs in behind the same
// interface. Grounding complements the context graph, which answers the
// relationship questions literal and semantic search cannot.
package grounder

import (
	"context"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"strings"
	"unicode/utf8"
)

// Ref is a grounded location.
type Ref struct {
	File string
	Line int
	Text string
}

// Grounder returns up to k locations relevant to a query.
type Grounder interface {
	Ground(ctx context.Context, query string, k int) ([]Ref, error)
}

// Nop grounds nothing; it is the zero grounder for projects that want none.
type Nop struct{}

var _ Grounder = Nop{}

// Ground returns no references.
func (Nop) Ground(context.Context, string, int) ([]Ref, error) { return nil, nil }

// Grep grounds by case-insensitive substring search under Root.
type Grep struct {
	Root string
}

var _ Grounder = Grep{}

// Ground walks Root and returns up to k lines containing the query.
func (g Grep) Ground(ctx context.Context, query string, k int) ([]Ref, error) {
	if query == "" || k <= 0 {
		return nil, nil
	}
	needle := strings.ToLower(query)
	var refs []Ref
	err := filepath.WalkDir(g.Root, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if ctx.Err() != nil {
			return ctx.Err()
		}
		if d.IsDir() {
			if name := d.Name(); name == ".git" || name == ".rigger" || name == "vendor" {
				return fs.SkipDir
			}
			return nil
		}
		b, readErr := os.ReadFile(path)
		if readErr != nil || !utf8.Valid(b) {
			return nil // skip unreadable or binary files
		}
		for i, line := range strings.Split(string(b), "\n") {
			if strings.Contains(strings.ToLower(line), needle) {
				rel, relErr := filepath.Rel(g.Root, path)
				if relErr != nil {
					rel = path
				}
				refs = append(refs, Ref{File: rel, Line: i + 1, Text: strings.TrimSpace(line)})
				if len(refs) >= k {
					return fs.SkipAll
				}
			}
		}
		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("grounder: walk: %w", err)
	}
	return refs, nil
}
