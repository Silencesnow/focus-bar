# Focus Bar cmux MVP Design

## Goal

Deliver a dependable, local macOS attention bar for cmux. The first milestone is complete when Focus Bar can:

1. connect to a running cmux instance from a standalone Tauri application;
2. show every currently open cmux workspace with an accurate attention-oriented status;
3. update promptly when cmux state changes;
4. jump to the correct cmux window and workspace when a task is clicked; and
5. explain connection and action failures instead of presenting them as an empty task list.

This milestone intentionally supports cmux only. General CLI adapters, workflow orchestration, Chrome/VS Code expansion, packaging, login-item startup, and cross-machine support are out of scope.

## Confirmed Decisions

- cmux socket access uses `allowAll` for this single-user Mac MVP.
- Focus Bar does not silently edit cmux configuration. It detects the access mode and presents setup or restart guidance when access is unavailable.
- The first version has four task states: `executing`, `needs_action`, `needs_review`, and `idle`.
- Manual status is retained as a temporary override. Clearing the override resumes automatic status calculation.
- Active cmux workspaces are the source of truth for the rendered task list. Historical records in `~/.focus.json` are preserved but not rendered when their workspace is absent.

## Considered Approaches

### A. Rust wrapper around the cmux CLI — selected

The Tauri backend resolves the cmux executable, invokes typed commands, owns the event-stream process, and returns structured results to the frontend.

This reuses cmux's supported CLI contract, fits the current codebase, and leaves a clean adapter boundary for future tools without requiring a generic plugin system now.

### B. Direct Unix-socket JSON RPC

This removes the executable-discovery problem and offers tighter streaming control, but Focus Bar would need to track the socket RPC protocol and authentication details itself. That maintenance cost is unnecessary for the first milestone.

### C. Bridge process spawned inside cmux

This could retain the default `cmuxOnly` access mode, but it couples Focus Bar startup and lifetime to a cmux terminal process. The accepted `allowAll` decision removes the main reason to accept that complexity.

## Architecture

The system is split into four units with narrow contracts.

### `CmuxRuntime` in Rust

Responsibilities:

- resolve the CLI and socket;
- run a startup health check;
- execute cmux commands with bounded timeouts;
- fetch a complete snapshot;
- own one reconnecting `cmux events` watcher;
- perform workspace focus and notification-read actions; and
- map process failures into stable error codes.

CLI resolution order:

1. `CMUX_BUNDLED_CLI_PATH` when set and executable;
2. `cmux` found on `PATH`;
3. `/Applications/cmux.app/Contents/Resources/bin/cmux`;
4. `~/Applications/cmux.app/Contents/Resources/bin/cmux`.

Socket resolution order:

1. `CMUX_SOCKET_PATH` when set;
2. the path stored in `~/.local/state/cmux/last-socket-path` when it names a socket;
3. cmux CLI auto-discovery.

When an explicit socket is found, every child command receives the same `CMUX_SOCKET_PATH`. Runtime discovery happens at command time or after a connection failure so a cmux restart does not permanently pin a stale socket.

### Snapshot and event boundary

The Rust API returns one structured snapshot instead of a JSON string:

```text
CmuxSnapshot {
  source: ready | error,
  workspaces: CmuxWorkspace[],
  notifications: CmuxNotification[],
  fetched_at: timestamp
}
```

The event watcher runs with subscription acknowledgements and heartbeat frames suppressed, then emits a lightweight `cmux-state-changed` signal for each real event. The frontend debounces signals for 250 ms and requests a fresh snapshot. A 30-second fallback refresh catches lost events and verifies source health. Only one refresh may be in flight; one additional refresh is queued if an event arrives during it.

The watcher is started once. If the process exits, Rust retries with exponential backoff capped at 15 seconds and emits source-health changes to the frontend.

### Frontend state derivation

Status calculation is a pure function over a workspace, its notifications, the current time, and an optional manual override. Priority is:

1. a non-null manual override;
2. an unread notification whose text indicates waiting, input required, blocked, error, or failure → `needs_action`;
3. an unread notification whose text indicates completed, done, success, or finished → `needs_review`;
4. `latest_submitted_at` newer than the newest terminal notification → `executing`;
5. otherwise → `idle`.

Matching is case-insensitive and examines title, subtitle, and body. Read terminal notifications no longer keep a task permanently blocked or awaiting review.

`needs_action` wins over `needs_review` when multiple unread notifications exist. Within a status, the newest notification supplies the displayed reason.

### Presentation and actions

The bar renders only workspaces present in the latest successful snapshot. Local task records contribute display name, note, and manual override, but do not create disconnected cards.

The source health state is separate from task state:

- `CLI_NOT_FOUND`: cmux is installed but its CLI cannot be resolved, or cmux is absent;
- `CMUX_NOT_RUNNING`: no live socket or ping response;
- `ACCESS_DENIED`: socket mode does not permit Focus Bar;
- `TIMEOUT`: cmux did not respond within the command deadline;
- `INVALID_RESPONSE`: output could not be decoded;
- `WATCHER_DISCONNECTED`: the last snapshot remains visible while event reconnection is attempted.

Initial source failure shows a diagnostic message and retry action, not an empty list. A refresh failure after a successful snapshot keeps the last good cards, marks them stale, and shows the source error.

Clicking a card performs one cmux jump transaction:

1. temporarily disable Focus Bar's always-on-top flag;
2. launch or activate cmux;
3. focus the recorded cmux window;
4. select the recorded workspace by its stable UUID, falling back to the snapshot ref only when no UUID is available;
5. mark that workspace's notifications read only after selection succeeds; and
6. restore Focus Bar's always-on-top flag in a `finally` path.

An action failure keeps notifications unread and shows the typed error. Explicit VS Code and Chrome jumps remain in the source but are not expanded in this milestone.

## Local Configuration and Data

The app's doctor view explains that cmux must allow local external clients and verifies the setting by capability or ping behavior. It does not rewrite `~/.config/cmux/cmux.json`.

Before integration testing, `automation.socketControlMode` must be set to `allowAll` through supported cmux configuration, followed by the reload or restart required by the installed cmux version. Any manual configuration edit must first create a timestamped backup.

`~/.focus.json` remains the storage location for user overrides in this milestone. Existing missing-workspace entries are ignored rather than deleted. This removes duplicate stale cards without destroying notes or configuration.

## Testing

### Pure status tests

Use Bun's test runner for table-driven fixtures covering:

- each automatic state;
- state priority with multiple notifications;
- case-insensitive title/subtitle/body matching;
- read versus unread terminal notifications;
- submitted-after-terminal execution inference;
- manual override and override clearing; and
- absent historical workspace records.

### Rust tests

Test executable and socket resolution with injected filesystem/environment inputs. Test command timeout, non-zero exit, malformed JSON, and typed error mapping without requiring a live cmux instance.

### Live integration test

Provide an opt-in local test that requires cmux and validates health, snapshot decoding, event-triggered refresh, and workspace selection. It must not mark notifications read unless the test explicitly requests the action phase.

### Manual acceptance

Run the Tauri app and verify:

1. current workspaces appear without duplicate historical cards;
2. waiting and completed notifications update the expected cards;
3. source failure produces a visible diagnostic;
4. restarting cmux recovers without restarting Focus Bar; and
5. clicking a card focuses the correct cmux window/workspace and clears its unread attention state.

## Delivery Sequence

1. Add pure status fixtures and failing tests.
2. Introduce the Rust `CmuxRuntime` health and snapshot boundary.
3. Switch the frontend to structured snapshots and the four-state reducer.
4. Connect the event watcher with fallback refresh and stale-state behavior.
5. Implement the safe jump transaction.
6. Hide absent historical workspace records and repair manual override semantics.
7. Update setup documentation and replace the misleading Python checks with targeted tests.
8. Complete live and manual acceptance against the installed cmux version.

## Success Criteria

The milestone is accepted when a standalone Focus Bar process can start outside cmux, report source health accurately, render and update current cmux workspace states, recover from a cmux restart, and jump to the selected cmux workspace without losing unread state on failure.
