# Chrome Instance Routing Design

## Context

Focus Bar currently sends a static AppleScript to `application "Google Chrome"`. This works when one Chrome instance is running, but macOS routes Apple Events to an arbitrary instance when multiple processes share the `com.google.Chrome` bundle identifier. In the current environment it selected a cmux-launched remote-debugging Chrome instead of the user's ordinary Chrome.

The selected behavior is: target the frontmost visible non-debug Chrome instance, match the configured URL exactly inside that process, and open a new tab in that same process only when no exact match exists.

## Considered Approaches

1. **Process-specific ScriptingBridge navigation (selected).** Discover ordinary Chrome PIDs, choose the frontmost visible instance, and connect through `SBApplication(processIdentifier:)`. This preserves URL-only configuration and handles multiple instances without a browser extension.
2. **Add a Chrome profile/instance field to task settings.** Explicit but exposes implementation details to the user, becomes stale when PIDs change, and does not match the requested “current Chrome” behavior.
3. **Build a Chrome extension plus native messaging host.** Offers excellent tab control but adds installation, distribution, permissions, and a persistent bridge that are disproportionate to the current local MVP.

## Architecture

### Instance discovery and selection

Rust runs the trusted system probe `/bin/ps -axo pid=,command=` and keeps only root Google Chrome processes whose executable is `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`. Processes whose arguments contain `--remote-debugging-port` are classified as debug instances and excluded.

Core Graphics returns the on-screen window list in front-to-back order. The first window owned by an eligible PID selects the current ordinary Chrome instance. If eligible instances exist but have no visible windows, Focus Bar selects the eligible root process with the lowest PID for deterministic behavior. If no ordinary instance exists, the command returns `CHROME_NOT_INSTALLED` with guidance to start the normal Google Chrome application; it does not silently navigate a debug instance.

The process discovery and selection logic is kept pure after system data is collected so it can be unit-tested with synthetic process and window-order fixtures.

### Process-specific tab navigation

The existing static-script boundary is preserved, but the Chrome script uses AppleScriptObjC's ScriptingBridge framework:

- Rust passes the selected PID and configured URL as `osascript` arguments.
- The script creates `SBApplication(processIdentifier:)` for that PID.
- It reads each window's tab URLs in one batch per window.
- Exact string equality selects a matching tab, moves its window to the front, and activates that Chrome process.
- If no exact match exists, it creates a tab in the selected instance and sets its URL. If the instance has no windows, it creates a window first.

User-provided URLs are never interpolated into script source.

### Timeouts and permissions

Every child command uses `kill_on_drop(true)`, so a timeout terminates the pending `osascript` instead of leaving a process or macOS consent dialog behind. Automation denials remain `AUTOMATION_PERMISSION_REQUIRED`. A genuine timeout remains `TARGET_TIMEOUT`, and the UI explains that an unhandled macOS permission dialog may be waiting.

The settings test action and overlay browser icon use the same command and therefore the same instance-selection behavior.

## Testing

- Unit tests cover debug-process exclusion, front-to-back eligible PID selection, deterministic hidden-window fallback, and no-eligible-instance errors.
- Static-script tests verify that PID and URL are arguments, tab URLs are read in batches, and user values never enter source.
- The timeout regression test proves the child process is terminated.
- Live acceptance runs with both the ordinary Chrome and cmux debug Chrome present: an existing exact URL must focus in the ordinary instance; a missing URL must open there; no test tab or permission process may remain afterward.

## Out of Scope

- Choosing a specific Chrome profile within one ordinary process.
- Supporting Chromium, Chrome Canary, Arc, Safari, or Edge.
- Installing a Chrome extension or native messaging host.
- Controlling remote-debugging Chrome instances.
