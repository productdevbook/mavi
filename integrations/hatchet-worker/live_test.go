package main

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/google/uuid"
	hatchet "github.com/hatchet-dev/hatchet/sdks/go"
)

// TestHatchetLiveDispatch is intentionally opt-in. CI runs it against the
// pinned self-hosted Hatchet service; ordinary package tests stay hermetic and
// do not require credentials or a running control plane.
func TestHatchetLiveDispatch(t *testing.T) {
	if os.Getenv("HATCHET_LIVE_E2E") != "1" {
		t.Skip("set HATCHET_LIVE_E2E=1 to run against a real Hatchet server")
	}

	const bridgeSecret = "live-e2e-bridge-secret"
	received := make(chan workflowIntent, 1)
	executor := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.URL.Path != "/internal/v1/workflows/execute" {
			http.NotFound(writer, request)
			return
		}
		if request.Header.Get("Authorization") != "Bearer "+bridgeSecret {
			http.Error(writer, "unauthorized", http.StatusUnauthorized)
			return
		}
		var intent workflowIntent
		if err := json.NewDecoder(request.Body).Decode(&intent); err != nil {
			http.Error(writer, "invalid intent", http.StatusBadRequest)
			return
		}
		select {
		case received <- intent:
		default:
		}
		writer.Header().Set("content-type", "application/json")
		_, _ = writer.Write([]byte(`{"run_id":"live-e2e"}`))
	}))
	defer executor.Close()

	_ = os.Setenv("MAVI_HATCHET_BRIDGE_SECRET", bridgeSecret)
	_ = os.Setenv("MAVI_RUST_EXECUTOR_URL", executor.URL)
	bridge, err := newBridge()
	if err != nil {
		t.Fatalf("create live bridge: %v", err)
	}
	if err := bridge.configureRateLimit(); err != nil {
		t.Fatalf("configure live rate limit: %v", err)
	}

	workflow := bridge.client.NewWorkflow(dispatchWorkflow)
	workflow.NewTask("execute-rust-intent", func(ctx hatchet.Context, input workflowIntent) (dispatchResponse, error) {
		return bridge.execute(ctx, input)
	}, hatchet.WithRetries(1), hatchet.WithExecutionTimeout(2*time.Minute))
	worker, err := bridge.client.NewWorker(
		"mavi-live-e2e-"+strings.ReplaceAll(uuid.NewString(), "-", ""),
		hatchet.WithWorkflows(workflow),
		hatchet.WithSlots(1),
	)
	if err != nil {
		t.Fatalf("create live worker: %v", err)
	}
	workerContext, cancelWorker := context.WithCancel(context.Background())
	defer cancelWorker()
	workerErrors := make(chan error, 1)
	go func() {
		workerErrors <- worker.StartBlocking(workerContext)
	}()

	intent := workflowIntent{
		ID:             uuid.NewString(),
		SiteID:         os.Getenv("MAVI_SITE_ID"),
		Plugin:         "core",
		Workflow:       "live.e2e",
		IdempotencyKey: "live-e2e:" + uuid.NewString(),
		Payload:        map[string]any{"source": "ci"},
	}
	if _, err := bridge.dispatchIntent(context.Background(), intent); err != nil {
		t.Fatalf("dispatch live workflow: %v", err)
	}

	select {
	case delivered := <-received:
		if delivered.ID != intent.ID || delivered.IdempotencyKey != intent.IdempotencyKey {
			t.Fatalf("Hatchet delivered a different intent: got %q/%q, want %q/%q", delivered.ID, delivered.IdempotencyKey, intent.ID, intent.IdempotencyKey)
		}
	case err := <-workerErrors:
		t.Fatalf("live Hatchet worker stopped before delivery: %v", err)
	case <-time.After(45 * time.Second):
		t.Fatal("timed out waiting for Hatchet to deliver the workflow to the Rust executor")
	}
}
