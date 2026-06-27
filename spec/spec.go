// Package spec extracts the enumerable acceptance criteria from a spec document.
// The conductor's coverage gate checks that every criterion is covered by a stage
// before it will call a run done (architecture: "done = every spec criterion
// covered + every unit integrated + every gate green").
package spec

import (
	"regexp"
	"strings"
)

// checkbox matches a markdown task item: "- [ ] text" or "* [x] text".
var checkbox = regexp.MustCompile(`^\s*[-*]\s*\[[ xX]\]\s+(.*\S)\s*$`)

// ExtractCriteria returns the acceptance criteria in a spec: the text of every
// checkbox item ("- [ ] ..."). These are the "Done-when" list the loop is
// machine-verified against; a spec with none is not loop-ready.
func ExtractCriteria(text string) []string {
	var out []string
	for _, line := range strings.Split(text, "\n") {
		if m := checkbox.FindStringSubmatch(line); m != nil {
			out = append(out, strings.TrimSpace(m[1]))
		}
	}
	return out
}
