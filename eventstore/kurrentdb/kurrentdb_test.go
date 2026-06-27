package kurrentdb_test

import (
	"context"
	"fmt"
	"sync/atomic"
	"testing"
	"time"

	"github.com/testcontainers/testcontainers-go"
	"github.com/testcontainers/testcontainers-go/wait"

	"github.com/virtual-velocitation/rigger/eventstore"
	"github.com/virtual-velocitation/rigger/eventstore/eventstoretest"
	"github.com/virtual-velocitation/rigger/eventstore/kurrentdb"
	"github.com/virtual-velocitation/rigger/eventstore/namespace"
)

// TestKurrentDBContract runs the backend-agnostic contract suite against a real
// KurrentDB started with testcontainers. One container is shared across the
// subtests; each subtest gets an isolated namespace (proven contract-transparent
// in the namespace package), so they don't interfere and we boot only once.
func TestKurrentDBContract(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping KurrentDB integration test in -short mode")
	}
	conn := startKurrentDB(t)

	var n atomic.Int64
	eventstoretest.RunContract(t, func(t *testing.T) eventstore.EventStore {
		t.Helper()
		store, err := kurrentdb.Open(conn)
		if err != nil {
			t.Fatalf("open KurrentDB store: %v", err)
		}
		t.Cleanup(func() { _ = store.Close() })
		return namespace.New(store, fmt.Sprintf("contract-%d", n.Add(1)))
	})
}

func startKurrentDB(t *testing.T) string {
	t.Helper()
	ctx := context.Background()
	req := testcontainers.ContainerRequest{
		Image:        "kurrentplatform/kurrentdb:latest",
		ExposedPorts: []string{"2113/tcp"},
		Cmd:          []string{"--insecure", "--run-projections=All", "--enable-atom-pub-over-http", "--mem-db"},
		WaitingFor:   wait.ForHTTP("/info").WithPort("2113/tcp").WithStartupTimeout(120 * time.Second),
	}
	container, err := testcontainers.GenericContainer(ctx, testcontainers.GenericContainerRequest{
		ContainerRequest: req,
		Started:          true,
	})
	if err != nil {
		t.Skipf("could not start a KurrentDB container (is a container runtime available?): %v", err)
	}
	t.Cleanup(func() { _ = container.Terminate(ctx) })

	host, err := container.Host(ctx)
	if err != nil {
		t.Fatalf("container host: %v", err)
	}
	port, err := container.MappedPort(ctx, "2113")
	if err != nil {
		t.Fatalf("mapped port: %v", err)
	}
	return fmt.Sprintf("kurrentdb://%s:%s?tls=false", host, port.Port())
}
