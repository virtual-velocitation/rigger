package safety_test

import (
	"testing"

	"github.com/virtual-velocitation/rigger/safety"
)

func TestBudgetExhausted(t *testing.T) {
	if !safety.BudgetExhausted(100, 100) {
		t.Error("spent == threshold should be exhausted")
	}
	if !safety.BudgetExhausted(100, 150) {
		t.Error("over threshold should be exhausted")
	}
	if safety.BudgetExhausted(100, 50) {
		t.Error("under threshold should not be exhausted")
	}
	if safety.BudgetExhausted(0, 999) {
		t.Error("a non-positive threshold means no limit")
	}
}

func TestRemediate(t *testing.T) {
	if r := safety.Remediate(0, safety.MaxRetries); r.Decision != safety.Retry || r.Attempts != 1 {
		t.Errorf("first failure should retry: %+v", r)
	}
	if r := safety.Remediate(1, safety.MaxRetries); r.Decision != safety.Retry || r.Attempts != 2 {
		t.Errorf("second failure should still retry: %+v", r)
	}
	if r := safety.Remediate(2, safety.MaxRetries); r.Decision != safety.Escalate || r.Attempts != 3 {
		t.Errorf("at the bound should escalate: %+v", r)
	}
}

func TestAbort(t *testing.T) {
	kept, discarded := safety.Abort([]safety.IntegratedID{
		{ID: "a", Integrated: true},
		{ID: "b", Integrated: false},
		{ID: "c", Integrated: true},
	})
	if len(kept) != 2 {
		t.Errorf("kept = %v, want 2 integrated", kept)
	}
	if len(discarded) != 1 || discarded[0] != "b" {
		t.Errorf("discarded = %v, want [b]", discarded)
	}
}
