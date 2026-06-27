package eventstore

import (
	"strings"
	"testing"
)

func TestConflictErrorMessage(t *testing.T) {
	err := &ConflictError{Stream: "run-7", Expected: NoStream, Actual: 3}
	msg := err.Error()
	for _, want := range []string{"run-7", "-1", "3"} {
		if !strings.Contains(msg, want) {
			t.Errorf("ConflictError message %q is missing %q", msg, want)
		}
	}
}

func TestSentinelsAreStable(t *testing.T) {
	// These values are part of the wire/contract; drifting them silently breaks
	// every backend and every caller.
	if NoStream != -1 {
		t.Errorf("NoStream = %d, want -1", NoStream)
	}
	if Any != -2 {
		t.Errorf("Any = %d, want -2", Any)
	}
	if Forward != 0 {
		t.Errorf("Forward should be the zero Direction, got %d", Forward)
	}
}
