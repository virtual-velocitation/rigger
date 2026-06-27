package gate

import (
	"context"
	"os/exec"
	"regexp"
	"strings"
)

// ExecRunner runs a gate's command through the shell and returns compact
// evidence. It is the production Runner; tests inject a stub.
type ExecRunner struct{}

var _ Runner = ExecRunner{}

// Run executes the gate's command in dir (empty means the current directory).
// Exit 0 is a pass.
func (ExecRunner) Run(ctx context.Context, g Gate, dir string) Result {
	cmd := exec.CommandContext(ctx, "sh", "-c", g.Run)
	cmd.Dir = dir
	out, err := cmd.CombinedOutput()
	return Result{Pass: err == nil, Evidence: compact(string(out), err == nil)}
}

const maxEvidence = 780

var failLine = regexp.MustCompile(`(?i)\b(fail|failed|error|panic)\b|✖`)

// compact reduces command output to a verdict plus a few salient lines, never a
// raw log. On failure it keeps the lines that look like failures (or the last
// few); on success it keeps a short tail.
func compact(out string, pass bool) string {
	verdict := "FAIL"
	if pass {
		verdict = "PASS"
	}
	lines := splitNonEmpty(out)
	var keep []string
	if pass {
		keep = lastN(lines, 1)
	} else {
		for _, l := range lines {
			if failLine.MatchString(l) {
				keep = append(keep, l)
			}
		}
		if len(keep) == 0 {
			keep = lastN(lines, 5)
		}
		if len(keep) > 5 {
			keep = keep[:5]
		}
	}
	ev := verdict
	if len(keep) > 0 {
		ev += ": " + strings.Join(keep, " | ")
	}
	if len(ev) > maxEvidence {
		ev = ev[:maxEvidence]
	}
	return ev
}

func splitNonEmpty(s string) []string {
	var out []string
	for _, l := range strings.Split(s, "\n") {
		if t := strings.TrimSpace(l); t != "" {
			out = append(out, t)
		}
	}
	return out
}

func lastN(lines []string, n int) []string {
	if len(lines) <= n {
		return lines
	}
	return lines[len(lines)-n:]
}
