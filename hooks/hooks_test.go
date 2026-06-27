package hooks_test

import (
	"encoding/json"
	"strings"
	"testing"

	"github.com/virtual-velocitation/rigger/hooks"
)

func TestInstallSessionStartFresh(t *testing.T) {
	out, err := hooks.InstallSessionStart(nil, "rigger prime")
	if err != nil {
		t.Fatalf("install: %v", err)
	}
	if !strings.Contains(string(out), "rigger prime") || !strings.Contains(string(out), "SessionStart") {
		t.Errorf("output missing the hook: %s", out)
	}
	if !json.Valid(out) {
		t.Error("output is not valid JSON")
	}
}

func TestInstallSessionStartIdempotent(t *testing.T) {
	once, err := hooks.InstallSessionStart(nil, "rigger prime")
	if err != nil {
		t.Fatal(err)
	}
	twice, err := hooks.InstallSessionStart(once, "rigger prime")
	if err != nil {
		t.Fatal(err)
	}
	if string(once) != string(twice) {
		t.Errorf("installing twice should be a no-op:\nonce:  %s\ntwice: %s", once, twice)
	}
	if n := strings.Count(string(twice), "rigger prime"); n != 1 {
		t.Errorf("hook appears %d times, want exactly 1", n)
	}
}

func TestInstallSessionStartPreservesExisting(t *testing.T) {
	existing := `{
	  "model": "opus",
	  "hooks": {
	    "SessionStart": [
	      {"matcher": "", "hooks": [{"type": "command", "command": "other-tool prime"}]}
	    ]
	  }
	}`
	out, err := hooks.InstallSessionStart([]byte(existing), "rigger prime")
	if err != nil {
		t.Fatalf("install: %v", err)
	}
	s := string(out)
	for _, want := range []string{"opus", "other-tool prime", "rigger prime"} {
		if !strings.Contains(s, want) {
			t.Errorf("merged settings should preserve/contain %q: %s", want, s)
		}
	}
}
