package sqlite_test

import (
	"path/filepath"
	"testing"

	"github.com/virtual-velocitation/rigger/eventstore"
	"github.com/virtual-velocitation/rigger/eventstore/eventstoretest"
	"github.com/virtual-velocitation/rigger/eventstore/sqlite"
)

// TestSQLiteContract proves the SQLite adapter honors the full EventStore
// contract. The same suite is what the future KurrentDB adapter must pass.
func TestSQLiteContract(t *testing.T) {
	eventstoretest.RunContract(t, func(t *testing.T) eventstore.EventStore {
		t.Helper()
		path := filepath.Join(t.TempDir(), "events.db")
		store, err := sqlite.Open(path)
		if err != nil {
			t.Fatalf("open sqlite store: %v", err)
		}
		t.Cleanup(func() { _ = store.Close() })
		return store
	})
}
