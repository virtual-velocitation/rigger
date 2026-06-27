package spec_test

import (
	"reflect"
	"testing"

	"github.com/virtual-velocitation/rigger/spec"
)

func TestExtractCriteria(t *testing.T) {
	text := `# Feature

Some prose that is not a criterion.

## Done when
- [ ] the event store passes the contract suite
- [x] the graph supports bi-temporal supersession
* [ ] the conductor integrates work into the repo

- a plain bullet, not a checkbox, is ignored
`
	got := spec.ExtractCriteria(text)
	want := []string{
		"the event store passes the contract suite",
		"the graph supports bi-temporal supersession",
		"the conductor integrates work into the repo",
	}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("ExtractCriteria =\n  %#v\nwant\n  %#v", got, want)
	}
}

func TestExtractCriteriaEmpty(t *testing.T) {
	if got := spec.ExtractCriteria("# No criteria here\n\njust prose"); len(got) != 0 {
		t.Errorf("expected no criteria, got %v", got)
	}
}
