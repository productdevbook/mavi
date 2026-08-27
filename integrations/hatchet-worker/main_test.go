package main

import (
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	hatchet "github.com/hatchet-dev/hatchet/sdks/go"
)

func TestConstantTimeBearer(t *testing.T) {
	t.Parallel()

	if !constantTimeBearer("Bearer secret", "secret") {
		t.Fatal("expected matching bearer token")
	}
	if constantTimeBearer("Bearer secret", "secret-extra") {
		t.Fatal("expected different-length token to be rejected")
	}
	if constantTimeBearer("Basic secret", "secret") {
		t.Fatal("expected non-bearer token to be rejected")
	}
}

func TestPriorityForIntent(t *testing.T) {
	t.Parallel()

	cases := []struct {
		plugin string
		want   hatchet.RunPriority
	}{
		{plugin: "commerce", want: hatchet.RunPriorityHigh},
		{plugin: "learning", want: hatchet.RunPriorityHigh},
		{plugin: "analytics", want: hatchet.RunPriorityLow},
		{plugin: "writing", want: hatchet.RunPriorityMedium},
	}
	for _, test := range cases {
		t.Run(test.plugin, func(t *testing.T) {
			t.Parallel()
			if got := priorityForIntent(workflowIntent{Plugin: test.plugin}); got != test.want {
				t.Fatalf("priority = %v, want %v", got, test.want)
			}
		})
	}
}

func TestDedupeKeyIsSiteScoped(t *testing.T) {
	t.Parallel()

	intent := workflowIntent{SiteID: "site-a", IdempotencyKey: "content:publish:42"}
	if got, want := dedupeKey(intent), "site-a:content:publish:42"; got != want {
		t.Fatalf("dedupe key = %q, want %q", got, want)
	}
	if dedupeKey(workflowIntent{SiteID: "site-b", IdempotencyKey: intent.IdempotencyKey}) == dedupeKey(intent) {
		t.Fatal("dedupe keys for different sites must not collide")
	}
}

func TestServeHTTPProtectsBridgeRoutes(t *testing.T) {
	t.Parallel()

	bridge := &bridge{secret: "secret"}
	request := httptest.NewRequest(http.MethodPost, "/internal/v1/workflows/dispatch", nil)
	response := httptest.NewRecorder()
	bridge.ServeHTTP(response, request)
	if response.Code != http.StatusUnauthorized {
		t.Fatalf("status = %d, want %d", response.Code, http.StatusUnauthorized)
	}
}

func TestScheduledAtOnlyAcceptsFutureRFC3339Payloads(t *testing.T) {
	t.Parallel()

	future := time.Now().UTC().Add(time.Minute).Format(time.RFC3339Nano)
	triggerAt, ok, err := scheduledAt(workflowIntent{Payload: map[string]any{"run_at": future}})
	if err != nil || !ok || !triggerAt.After(time.Now().UTC()) {
		t.Fatalf("future run_at = %v, %v, %v", triggerAt, ok, err)
	}

	if _, ok, err := scheduledAt(workflowIntent{Payload: map[string]any{"run_at": nil}}); err != nil || ok {
		t.Fatalf("nil run_at = %v, %v; want no schedule", ok, err)
	}
	if _, ok, err := scheduledAt(workflowIntent{Payload: map[string]any{"run_at": "not-a-time"}}); err == nil || ok {
		t.Fatalf("invalid run_at = %v, %v; want an error", ok, err)
	}
}
