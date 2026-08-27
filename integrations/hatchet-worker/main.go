// Mavi's Hatchet adapter.
//
// This process owns the official Hatchet Go SDK and worker registration. It
// contains no Mavi business logic: every task forwards the small workflow
// intent to Rust over a private authenticated HTTP boundary.
package main

import (
	"context"
	"crypto/subtle"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
	"strconv"
	"strings"
	"time"

	"github.com/google/uuid"
	v0client "github.com/hatchet-dev/hatchet/pkg/client"
	"github.com/hatchet-dev/hatchet/pkg/client/rest"
	"github.com/hatchet-dev/hatchet/pkg/client/types"
	"github.com/hatchet-dev/hatchet/pkg/cmdutils"
	hatchet "github.com/hatchet-dev/hatchet/sdks/go"
	"github.com/hatchet-dev/hatchet/sdks/go/features"
	openapi_types "github.com/oapi-codegen/runtime/types"
)

const dispatchWorkflow = "mavi.dispatch"

const bridgeRateLimitKey = "mavi-rust-executor"

// Mail delivery uses the same durable task as every other Mavi workflow. Its
// provider-neutral DB row caps delivery attempts at 25, so Hatchet must be
// allowed to carry that retry budget instead of falling back to the removed
// Rust polling loop.
const maxTaskRetries = 25

type workflowIntent struct {
	ID             string         `json:"id"`
	SiteID         string         `json:"site_id"`
	Plugin         string         `json:"plugin"`
	Workflow       string         `json:"workflow"`
	IdempotencyKey string         `json:"idempotency_key"`
	Payload        map[string]any `json:"payload"`
}

type dispatchResponse struct {
	RunID string `json:"run_id"`
}

type bridge struct {
	client       *hatchet.Client
	secret       string
	executorURL  string
	tenantID     uuid.UUID
	siteID       string
	listen       string
	workflowName string
}

func main() {
	b, err := newBridge()
	if err != nil {
		log.Fatal(err)
	}
	if err := b.configureRateLimit(); err != nil {
		log.Fatalf("configure Hatchet rate limit: %v", err)
	}

	workflow := b.client.NewWorkflow(dispatchWorkflow,
		hatchet.WithWorkflowConcurrency(types.Concurrency{
			Expression: "input.plugin",
			MaxRuns:    int32Ptr(16),
		}),
	)
	workflow.NewTask("execute-rust-intent", func(ctx hatchet.Context, input workflowIntent) (dispatchResponse, error) {
		return b.execute(ctx, input)
	},
		hatchet.WithRetries(maxTaskRetries),
		hatchet.WithRetryBackoff(2.0, 60),
		hatchet.WithExecutionTimeout(10*time.Minute),
		hatchet.WithRateLimits(&types.RateLimit{Key: bridgeRateLimitKey, Units: intPtr(1)}),
	)

	worker, err := b.client.NewWorker(
		"mavi-hatchet-bridge",
		hatchet.WithWorkflows(workflow),
		hatchet.WithSlots(32),
	)
	if err != nil {
		log.Fatalf("create Hatchet worker: %v", err)
	}
	if err := b.configureMaintenanceCron(context.Background()); err != nil {
		log.Fatalf("configure Hatchet maintenance cron: %v", err)
	}

	server := &http.Server{
		Addr:              b.listen,
		Handler:           b,
		ReadHeaderTimeout: 5 * time.Second,
		ReadTimeout:       15 * time.Second,
		WriteTimeout:      15 * time.Second,
		MaxHeaderBytes:    16 << 10,
	}
	go func() {
		log.Printf("Mavi Hatchet bridge listening on %s", b.listen)
		if err := server.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
			log.Fatalf("bridge HTTP server: %v", err)
		}
	}()

	interruptCtx, cancel := cmdutils.NewInterruptContext()
	defer cancel()
	if err := worker.StartBlocking(interruptCtx); err != nil {
		log.Fatalf("Hatchet worker: %v", err)
	}
}

func newBridge() (*bridge, error) {
	secret := strings.TrimSpace(os.Getenv("MAVI_HATCHET_BRIDGE_SECRET"))
	if secret == "" {
		return nil, errors.New("MAVI_HATCHET_BRIDGE_SECRET is required")
	}
	executorURL := strings.TrimRight(strings.TrimSpace(os.Getenv("MAVI_RUST_EXECUTOR_URL")), "/")
	if executorURL == "" {
		return nil, errors.New("MAVI_RUST_EXECUTOR_URL is required")
	}
	token := strings.TrimSpace(os.Getenv("MAVI_HATCHET_TOKEN"))
	if token == "" {
		return nil, errors.New("MAVI_HATCHET_TOKEN is required")
	}
	hostPort := strings.TrimSpace(os.Getenv("MAVI_HATCHET_GRPC_ADDRESS"))
	if hostPort == "" {
		hostPort = "hatchet:7077"
	}
	serverURL := strings.TrimRight(strings.TrimSpace(os.Getenv("MAVI_HATCHET_SERVER_URL")), "/")
	if serverURL == "" {
		serverURL = "http://hatchet:8888"
	}
	tlsStrategy := envOr("MAVI_HATCHET_TLS_STRATEGY", "none")
	switch tlsStrategy {
	case "none", "tls", "mtls":
	default:
		return nil, errors.New("MAVI_HATCHET_TLS_STRATEGY must be none, tls, or mtls")
	}
	namespace := strings.TrimSpace(os.Getenv("MAVI_HATCHET_NAMESPACE"))
	if namespace == "" {
		namespace = "mavi"
	}
	tenantID := strings.TrimSpace(os.Getenv("MAVI_HATCHET_TENANT_ID"))
	if tenantID == "" {
		return nil, errors.New("MAVI_HATCHET_TENANT_ID is required")
	}
	tenantUUID, err := uuid.Parse(tenantID)
	if err != nil {
		return nil, fmt.Errorf("MAVI_HATCHET_TENANT_ID is not a UUID: %w", err)
	}
	siteID := strings.TrimSpace(os.Getenv("MAVI_SITE_ID"))
	if siteID != "" {
		if _, err := uuid.Parse(siteID); err != nil {
			return nil, fmt.Errorf("MAVI_SITE_ID is not a UUID: %w", err)
		}
	}
	// The v1 SDK's public constructor reads the transport endpoints from its
	// standard environment names. Set them once at this isolated process
	// boundary; the secret remains private to the bridge process.
	_ = os.Setenv("HATCHET_CLIENT_TENANT_ID", tenantID)
	_ = os.Setenv("HATCHET_CLIENT_TOKEN", token)
	_ = os.Setenv("HATCHET_CLIENT_HOST_PORT", hostPort)
	_ = os.Setenv("HATCHET_CLIENT_SERVER_URL", serverURL)
	_ = os.Setenv("HATCHET_CLIENT_TLS_STRATEGY", tlsStrategy)
	client, err := hatchet.NewClient(
		v0client.WithToken(token),
		v0client.WithNamespace(namespace),
	)
	if err != nil {
		return nil, fmt.Errorf("create Hatchet client: %w", err)
	}
	return &bridge{
		client:       client,
		secret:       secret,
		executorURL:  executorURL,
		tenantID:     tenantUUID,
		siteID:       siteID,
		listen:       envOr("MAVI_HATCHET_BRIDGE_LISTEN", "0.0.0.0:8090"),
		workflowName: dispatchWorkflow,
	}, nil
}

func (b *bridge) ServeHTTP(writer http.ResponseWriter, request *http.Request) {
	if request.Method != http.MethodPost {
		http.NotFound(writer, request)
		return
	}
	if !constantTimeBearer(request.Header.Get("Authorization"), b.secret) {
		http.Error(writer, "unauthorized", http.StatusUnauthorized)
		return
	}
	if request.URL.Path == "/internal/v1/workflows/cancel" {
		b.controlRun(writer, request, false)
		return
	}
	if request.URL.Path == "/internal/v1/workflows/replay" {
		b.controlRun(writer, request, true)
		return
	}
	if request.URL.Path != "/internal/v1/workflows/dispatch" {
		http.NotFound(writer, request)
		return
	}
	var intent workflowIntent
	decoder := json.NewDecoder(io.LimitReader(request.Body, 64<<10))
	if err := decoder.Decode(&intent); err != nil || intent.ID == "" || intent.Workflow == "" || intent.IdempotencyKey == "" {
		http.Error(writer, "invalid workflow intent", http.StatusBadRequest)
		return
	}
	dispatched, err := b.dispatchIntent(request.Context(), intent)
	if err != nil {
		http.Error(writer, "Hatchet rejected workflow", http.StatusBadGateway)
		return
	}
	writer.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(writer).Encode(dispatched)
}

func (b *bridge) dispatchIntent(ctx context.Context, intent workflowIntent) (dispatchResponse, error) {
	metadata := map[string]interface{}{
		"mavi_intent_id":         intent.ID,
		"mavi_idempotency_key":   intent.IdempotencyKey,
		"mavi_site_id":           intent.SiteID,
		"mavi_business_workflow": intent.Workflow,
		// Hatchet v0.71.14 consumes additionalMetadata["dedupe"] as its
		// native dedupe key. The local workflow row and Rust execution fence
		// remain the durable authority because a relay crash can create a run
		// before Hatchet finishes asynchronous dedupe handling, and because
		// they protect side effects if an external deployment changes policy.
		"dedupe": dedupeKey(intent),
	}
	priority := priorityForIntent(intent)
	if triggerAt, ok, err := scheduledAt(intent); err != nil {
		return dispatchResponse{}, err
	} else if ok {
		if existing, lookupErr := b.findExistingSchedule(ctx, intent); lookupErr == nil && existing != "" {
			return dispatchResponse{RunID: existing}, nil
		} else if lookupErr != nil {
			return dispatchResponse{}, fmt.Errorf("resolve scheduled Hatchet run: %w", lookupErr)
		}
		scheduled, err := b.client.Schedules().Create(ctx, b.workflowName, features.CreateScheduledRunTrigger{
			TriggerAt:          triggerAt,
			Input:              workflowInput(intent),
			AdditionalMetadata: metadata,
			Priority:           int32Ptr(int32(priority)),
		})
		if err != nil {
			return dispatchResponse{}, err
		}
		if scheduled == nil || scheduled.Metadata.Id == "" {
			return dispatchResponse{}, errors.New("Hatchet returned an empty schedule ID")
		}
		// A schedule has a different lifecycle from a workflow run. The prefix
		// lets cancel/replay route to the correct Hatchet API while retaining a
		// single local hatchet_run_id column.
		return dispatchResponse{RunID: "schedule:" + scheduled.Metadata.Id}, nil
	}

	run, err := b.client.RunNoWait(ctx, b.workflowName, intent,
		hatchet.WithRunMetadata(map[string]string{
			"mavi_intent_id":         intent.ID,
			"mavi_idempotency_key":   intent.IdempotencyKey,
			"mavi_site_id":           intent.SiteID,
			"mavi_business_workflow": intent.Workflow,
			"dedupe":                 dedupeKey(intent),
		}),
		hatchet.WithPriority(priority),
	)
	if err != nil {
		var dedupeErr *v0client.DedupeViolationErr
		if errors.As(err, &dedupeErr) {
			if existing, lookupErr := b.findExistingRun(ctx, intent); lookupErr == nil && existing != "" {
				return dispatchResponse{RunID: existing}, nil
			} else if lookupErr != nil {
				return dispatchResponse{}, fmt.Errorf("resolve deduplicated Hatchet run: %w", lookupErr)
			}
			return dispatchResponse{}, errors.New("Hatchet deduplicated workflow but the existing run was not visible")
		}
		return dispatchResponse{}, err
	}
	return dispatchResponse{RunID: run.RunId}, nil
}

func (b *bridge) findExistingSchedule(ctx context.Context, intent workflowIntent) (string, error) {
	rows, err := b.client.Schedules().List(ctx, rest.WorkflowScheduledListParams{})
	if err != nil {
		return "", err
	}
	if rows == nil || rows.Rows == nil {
		return "", nil
	}
	for _, row := range *rows.Rows {
		if row.AdditionalMetadata == nil {
			continue
		}
		value, ok := (*row.AdditionalMetadata)["dedupe"]
		if !ok || value != dedupeKey(intent) {
			continue
		}
		if row.WorkflowRunId != nil {
			return row.WorkflowRunId.String(), nil
		}
		return "schedule:" + row.Metadata.Id, nil
	}
	return "", nil
}

// findExistingRun handles a dedupe response from a Hatchet deployment that
// applies a server-side dedupe rule. WithRunMetadata is the supported native
// dedupe input in v0.71.14; this lookup is still a recovery path for a race or
// deployment that accepts the request before it finishes dedupe processing.
// Rust's local workflow fence makes any duplicate delivery a safe no-op.
func (b *bridge) findExistingRun(ctx context.Context, intent workflowIntent) (string, error) {
	metadata := fmt.Sprintf(`{"dedupe":%q}`, dedupeKey(intent))
	limit := int64(20)
	result, err := b.client.Runs().List(ctx, rest.V1WorkflowRunListParams{
		Since:              time.Unix(0, 0).UTC(),
		Limit:              &limit,
		AdditionalMetadata: &[]string{metadata},
		IncludePayloads:    boolPtr(false),
	})
	if err != nil {
		return "", err
	}
	if result == nil || result.JSON200 == nil {
		if result == nil {
			return "", errors.New("Hatchet run lookup returned no response")
		}
		return "", fmt.Errorf("Hatchet run lookup returned status %d", result.StatusCode())
	}
	for _, row := range result.JSON200.Rows {
		if row.AdditionalMetadata == nil {
			continue
		}
		if value, ok := (*row.AdditionalMetadata)["dedupe"]; ok && value == dedupeKey(intent) {
			return row.WorkflowRunExternalId.String(), nil
		}
	}
	return "", nil
}

func dedupeKey(intent workflowIntent) string {
	return intent.SiteID + ":" + intent.IdempotencyKey
}

func workflowInput(intent workflowIntent) map[string]interface{} {
	return map[string]interface{}{
		"id":              intent.ID,
		"site_id":         intent.SiteID,
		"plugin":          intent.Plugin,
		"workflow":        intent.Workflow,
		"idempotency_key": intent.IdempotencyKey,
		"payload":         intent.Payload,
	}
}

func scheduledAt(intent workflowIntent) (time.Time, bool, error) {
	raw, exists := intent.Payload["run_at"]
	if !exists || raw == nil {
		return time.Time{}, false, nil
	}
	value, ok := raw.(string)
	if !ok || strings.TrimSpace(value) == "" {
		return time.Time{}, false, errors.New("workflow run_at must be an RFC3339 string")
	}
	triggerAt, err := time.Parse(time.RFC3339Nano, value)
	if err != nil {
		return time.Time{}, false, fmt.Errorf("workflow run_at is invalid: %w", err)
	}
	return triggerAt, triggerAt.After(time.Now().UTC()), nil
}

type runControlRequest struct {
	RunID string `json:"run_id"`
}

type runControlResponse struct {
	RunID string `json:"run_id,omitempty"`
}

func (b *bridge) controlRun(writer http.ResponseWriter, request *http.Request, replay bool) {
	var input runControlRequest
	decoder := json.NewDecoder(io.LimitReader(request.Body, 8<<10))
	if err := decoder.Decode(&input); err != nil {
		http.Error(writer, "invalid run control request", http.StatusBadRequest)
		return
	}
	var resultingRunID string
	if strings.HasPrefix(input.RunID, "schedule:") {
		var err error
		resultingRunID, err = b.controlScheduled(request.Context(), strings.TrimPrefix(input.RunID, "schedule:"), replay)
		if err != nil {
			http.Error(writer, "Hatchet schedule control failed", http.StatusBadGateway)
			return
		}
	} else {
		runID, err := uuid.Parse(input.RunID)
		if err != nil {
			http.Error(writer, "invalid run id", http.StatusBadRequest)
			return
		}
		ids := []openapi_types.UUID{runID}
		if replay {
			_, err = b.client.Runs().Replay(request.Context(), rest.V1ReplayTaskRequest{ExternalIds: &ids})
		} else {
			_, err = b.client.Runs().Cancel(request.Context(), rest.V1CancelTaskRequest{ExternalIds: &ids})
		}
		if err != nil {
			http.Error(writer, "Hatchet run control failed", http.StatusBadGateway)
			return
		}
		resultingRunID = input.RunID
	}
	if resultingRunID != "" {
		writer.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(writer).Encode(runControlResponse{RunID: resultingRunID})
		return
	}
	writer.WriteHeader(http.StatusNoContent)
}

func (b *bridge) controlScheduled(ctx context.Context, scheduleID string, replay bool) (string, error) {
	scheduled, err := b.client.Schedules().Get(ctx, scheduleID)
	if err != nil {
		return "", err
	}
	if scheduled.WorkflowRunId != nil {
		ids := []openapi_types.UUID{*scheduled.WorkflowRunId}
		if replay {
			_, err = b.client.Runs().Replay(ctx, rest.V1ReplayTaskRequest{ExternalIds: &ids})
		} else {
			_, err = b.client.Runs().Cancel(ctx, rest.V1CancelTaskRequest{ExternalIds: &ids})
		}
		if err != nil {
			return "", err
		}
		return scheduled.WorkflowRunId.String(), nil
	}
	if !replay {
		return "", b.client.Schedules().Delete(ctx, scheduleID)
	}
	// A not-yet-fired schedule has no workflow run to replay. Recreate the
	// same input as an immediate scheduled run, then remove the old trigger.
	input := map[string]interface{}{}
	if scheduled.Input != nil {
		input = *scheduled.Input
	}
	metadata := map[string]interface{}{}
	if scheduled.AdditionalMetadata != nil {
		metadata = *scheduled.AdditionalMetadata
	}
	created, err := b.client.Schedules().Create(ctx, scheduled.WorkflowName, features.CreateScheduledRunTrigger{
		TriggerAt:          time.Now().UTC().Add(time.Second),
		Input:              input,
		AdditionalMetadata: metadata,
		Priority:           scheduled.Priority,
	})
	if err != nil {
		return "", err
	}
	if created == nil || created.Metadata.Id == "" {
		return "", errors.New("Hatchet returned an empty replay schedule ID")
	}
	if err := b.client.Schedules().Delete(ctx, scheduleID); err != nil {
		return "", err
	}
	return "schedule:" + created.Metadata.Id, nil
}

func (b *bridge) execute(ctx hatchet.Context, intent workflowIntent) (dispatchResponse, error) {
	body, err := json.Marshal(intent)
	if err != nil {
		return dispatchResponse{}, err
	}
	request, err := http.NewRequestWithContext(ctx, http.MethodPost, b.executorURL+"/internal/v1/workflows/execute", strings.NewReader(string(body)))
	if err != nil {
		return dispatchResponse{}, err
	}
	request.Header.Set("Content-Type", "application/json")
	request.Header.Set("Authorization", "Bearer "+b.secret)
	response, err := http.DefaultClient.Do(request)
	if err != nil {
		return dispatchResponse{}, err
	}
	defer response.Body.Close()
	if response.StatusCode < 200 || response.StatusCode >= 300 {
		return dispatchResponse{}, fmt.Errorf("Rust executor returned %s", response.Status)
	}
	return dispatchResponse{RunID: intent.ID}, nil
}

// configureRateLimit creates the shared bridge limit in Hatchet. The limit is
// intentionally outside Rust: every Mavi instance may point at a central
// bridge, so Hatchet must be the authority that meters calls to Rust.
func (b *bridge) configureRateLimit() error {
	limit, err := strconv.Atoi(envOr("MAVI_HATCHET_RATE_LIMIT_PER_MINUTE", "60"))
	if err != nil || limit < 1 || limit > 1_000_000 {
		return errors.New("MAVI_HATCHET_RATE_LIMIT_PER_MINUTE must be between 1 and 1000000")
	}
	return b.client.RateLimits().Upsert(features.CreateRatelimitOpts{
		Key:      bridgeRateLimitKey,
		Limit:    limit,
		Duration: types.Minute,
	})
}

// configureMaintenanceCron installs one site-scoped Hatchet cron when this
// bridge belongs to a Mavi instance. A central bridge can omit MAVI_SITE_ID
// and manage crons per instance through its control plane instead.
func (b *bridge) configureMaintenanceCron(ctx context.Context) error {
	if b.siteID == "" {
		return nil
	}
	expression := envOr("MAVI_HATCHET_MAINTENANCE_CRON", "*/5 * * * *")
	if !features.ValidateCronExpression(expression) {
		return errors.New("MAVI_HATCHET_MAINTENANCE_CRON is invalid")
	}
	name := "mavi-maintenance-" + strings.ReplaceAll(b.siteID, "-", "")
	rows, err := b.client.Crons().List(ctx, rest.CronWorkflowListParams{
		CronName: &name,
		Limit:    int64Ptr(10),
	})
	if err != nil {
		return err
	}
	if rows != nil && rows.Rows != nil && len(*rows.Rows) > 0 {
		return nil
	}
	_, err = b.client.Crons().Create(ctx, b.workflowName, features.CreateCronTrigger{
		Name:       name,
		Expression: expression,
		Input: workflowInput(workflowIntent{
			ID:             uuid.New().String(),
			SiteID:         b.siteID,
			Plugin:         "core",
			Workflow:       "maintenance.tick",
			IdempotencyKey: "maintenance:" + b.siteID,
			Payload:        map[string]interface{}{},
		}),
		AdditionalMetadata: map[string]interface{}{
			"mavi_site_id":  b.siteID,
			"mavi_workflow": "maintenance.tick",
		},
		Priority: int32Ptr(int32(hatchet.RunPriorityLow)),
	})
	return err
}

func priorityForIntent(intent workflowIntent) hatchet.RunPriority {
	switch intent.Plugin {
	case "commerce", "learning":
		return hatchet.RunPriorityHigh
	case "analytics", "governance":
		return hatchet.RunPriorityLow
	default:
		return hatchet.RunPriorityMedium
	}
}

func constantTimeBearer(value, secret string) bool {
	expected := "Bearer " + secret
	if len(value) != len(expected) {
		return false
	}
	return subtle.ConstantTimeCompare([]byte(value), []byte(expected)) == 1
}

func envOr(name, fallback string) string {
	if value := strings.TrimSpace(os.Getenv(name)); value != "" {
		return value
	}
	return fallback
}

func int32Ptr(value int32) *int32 { return &value }

func intPtr(value int) *int { return &value }

func int64Ptr(value int64) *int64 { return &value }

func boolPtr(value bool) *bool { return &value }
