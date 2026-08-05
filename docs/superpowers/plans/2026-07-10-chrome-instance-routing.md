# Chrome Instance Routing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Route exact Chrome URL jumps to the frontmost ordinary Google Chrome process instead of a cmux-launched remote-debugging instance.

**Architecture:** Rust discovers root Chrome processes from `/bin/ps`, excludes `--remote-debugging-port` instances, and combines the remaining PIDs with Core Graphics front-to-back window order. A static AppleScriptObjC script receives the selected PID and URL as arguments, connects with `SBApplication(processIdentifier:)`, focuses exact matches, and activates the same PID through AppKit. On `NOT_FOUND`, Rust invokes the official Chrome executable with `--new-tab <url>` so the ordinary default session opens the target while the cmux debug instance remains untouched.

**Tech Stack:** Rust 2021, Tokio, core-foundation 0.10, core-graphics 0.25, AppleScriptObjC, ScriptingBridge, Tauri 2, TypeScript 5.6, Bun.

## Global Constraints

- Work directly in the current `main` workspace; do not create a git worktree.
- Support `/Applications/Google Chrome.app` only.
- Never select a root process whose arguments contain `--remote-debugging-port`.
- Match Chrome tab URLs using exact string equality.
- Pass PID and URL through process arguments; never interpolate them into script source.
- If no ordinary Chrome process exists, return a typed error rather than using a debug instance.
- Preserve the existing 8-second timeout and terminate timed-out child processes.
- Keep VS Code navigation behavior unchanged.

---

### Task 1: Navigation command lifecycle and timeout guidance

**Files:**
- Modify: `src-tauri/src/navigation.rs`
- Modify: `src/navigation-config.ts`
- Modify: `src/navigation-config.test.ts`

**Interfaces:**
- Produces: `run_command_with_timeout(command, label, duration)` for command-level regression tests.
- Preserves: `run_command(command, label)` as the 8-second production wrapper.

- [ ] **Step 1: Add the failing child-termination regression test**

Add a test that starts `/bin/sh`, records its PID, replaces itself with `/bin/sleep 5`, and calls the not-yet-defined helper with a 200 ms timeout:

```rust
#[test]
fn timed_out_command_is_terminated() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let pid_path = std::env::temp_dir().join(format!(
            "focus-bar-timeout-{}-{}.pid",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg(format!(
            "echo $$ > '{}'; exec /bin/sleep 5",
            pid_path.display()
        ));

        let error = run_command_with_timeout(
            &mut command,
            "timeout test",
            Duration::from_millis(200),
        ).await.unwrap_err();
        assert_eq!(error.code, NavigationErrorCode::TargetTimeout);

        sleep(Duration::from_millis(100)).await;
        let pid = std::fs::read_to_string(&pid_path).unwrap();
        let still_running = std::process::Command::new("/bin/kill")
            .args(["-0", pid.trim()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status().unwrap().success();
        let _ = std::fs::remove_file(pid_path);
        assert!(!still_running, "timed out child process {pid} is still running");
    });
}
```

- [ ] **Step 2: Run the test to verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml navigation::tests::timed_out_command_is_terminated -- --exact`

Expected: compilation fails because `run_command_with_timeout` does not exist.

- [ ] **Step 3: Implement timeout cancellation**

```rust
async fn run_command(
    command: &mut Command,
    label: &str,
) -> Result<std::process::Output, NavigationError> {
    run_command_with_timeout(command, label, TARGET_TIMEOUT).await
}

async fn run_command_with_timeout(
    command: &mut Command,
    label: &str,
    duration: Duration,
) -> Result<std::process::Output, NavigationError> {
    command.kill_on_drop(true);
    timeout(duration, command.output())
        .await
        .map_err(|_| NavigationError::new(
            NavigationErrorCode::TargetTimeout,
            format!("{label} timed out"),
            None,
        ))?
        .map_err(|error| NavigationError::new(
            NavigationErrorCode::TargetCommandFailed,
            format!("Failed to start {label}"),
            Some(error.to_string()),
        ))
}
```

- [ ] **Step 4: Add timeout-copy RED/GREEN test**

Add:

```ts
test("timeout guidance mentions a possible permission prompt", () => {
  expect(navigationErrorMessage({ code: "TARGET_TIMEOUT", message: "timed out" }))
    .toContain("权限弹窗");
});
```

Run `bun test src/navigation-config.test.ts`, verify the new test fails, then change the mapping to:

```ts
case "TARGET_TIMEOUT":
  return "目标应用响应超时；请检查是否有等待处理的 macOS 权限弹窗。";
```

Run `bun test src/navigation-config.test.ts` again and expect 10 passing tests.

- [ ] **Step 5: Verify and commit**

Run: `cargo test --manifest-path src-tauri/Cargo.toml navigation::tests::timed_out_command_is_terminated -- --exact && bun test src/navigation-config.test.ts`

Expected: both targeted suites exit 0; the Rust test prints no `kill` warning.

Commit:

```bash
git add src-tauri/src/navigation.rs src/navigation-config.ts src/navigation-config.test.ts
git commit -m "fix: terminate timed-out navigation commands"
```

### Task 2: Ordinary Chrome process selection

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Modify: `src-tauri/src/navigation.rs`

**Interfaces:**
- Produces: `ChromeProcess { pid: u32, command: String }`.
- Produces: `parse_chrome_processes(output: &str) -> Vec<ChromeProcess>`.
- Produces: `select_chrome_pid(processes: &[ChromeProcess], window_order: &[u32]) -> Option<u32>`.
- Produces: `chrome_window_order() -> Vec<u32>` using Core Graphics.
- Consumes: `/bin/ps -axo pid=,command=` output.

- [ ] **Step 1: Write failing pure selection tests**

```rust
#[test]
fn chrome_process_parser_excludes_remote_debugging() {
    let output = "712 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome\n\
                  1899 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome --remote-debugging-port=9222 --user-data-dir=/tmp/debug\n\
                  1905 /Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Helper.app/Contents/MacOS/Google Chrome Helper --type=gpu-process";
    assert_eq!(parse_chrome_processes(output), vec![ChromeProcess {
        pid: 712,
        command: "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome".into(),
    }]);
}

#[test]
fn frontmost_eligible_window_wins() {
    let processes = vec![chrome_process(100), chrome_process(200)];
    assert_eq!(select_chrome_pid(&processes, &[999, 200, 100]), Some(200));
}

#[test]
fn hidden_windows_fall_back_to_lowest_pid() {
    let processes = vec![chrome_process(200), chrome_process(100)];
    assert_eq!(select_chrome_pid(&processes, &[]), Some(100));
}

#[test]
fn debug_only_processes_produce_no_target() {
    let output = "1899 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome --remote-debugging-port=9222";
    assert_eq!(select_chrome_pid(&parse_chrome_processes(output), &[]), None);
}
```

- [ ] **Step 2: Run tests to verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml navigation::tests::`

Expected: compilation fails because the process type and functions do not exist.

- [ ] **Step 3: Add Core Graphics dependencies and pure selection**

Add to `src-tauri/Cargo.toml`:

```toml
[target.'cfg(target_os = "macos")'.dependencies]
core-foundation = "0.10"
core-graphics = "0.25"
```

Implement:

```rust
const CHROME_EXECUTABLE: &str =
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

#[derive(Debug, Clone, PartialEq)]
struct ChromeProcess { pid: u32, command: String }

fn parse_chrome_processes(output: &str) -> Vec<ChromeProcess> {
    output.lines().filter_map(|line| {
        let line = line.trim();
        let split = line.find(char::is_whitespace)?;
        let pid = line[..split].parse().ok()?;
        let command = line[split..].trim();
        if !command.starts_with(CHROME_EXECUTABLE)
            || command.as_bytes().get(CHROME_EXECUTABLE.len()).is_some_and(|c| !c.is_ascii_whitespace())
            || command.contains("--remote-debugging-port")
        {
            return None;
        }
        Some(ChromeProcess { pid, command: command.into() })
    }).collect()
}

fn select_chrome_pid(processes: &[ChromeProcess], window_order: &[u32]) -> Option<u32> {
    let eligible: std::collections::HashSet<u32> =
        processes.iter().map(|process| process.pid).collect();
    window_order.iter().copied().find(|pid| eligible.contains(pid))
        .or_else(|| processes.iter().map(|process| process.pid).min())
}
```

- [ ] **Step 4: Implement system adapters**

Use `run_command` for `/bin/ps -axo pid=,command=`. Implement `chrome_window_order` with `core_graphics::window::copy_window_info`, typed `CFDictionary<CFString, CFType>`, `kCGWindowOwnerPID`, and `CFNumber::to_i64()`. Preserve the array's original front-to-back order and de-duplicate PIDs without sorting:

```rust
#[cfg(target_os = "macos")]
fn chrome_window_order() -> Vec<u32> {
    use core_foundation::array::CFArray;
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use core_graphics::window::{
        copy_window_info, kCGNullWindowID, kCGWindowListExcludeDesktopElements,
        kCGWindowListOptionOnScreenOnly, kCGWindowOwnerPID,
    };

    let options = kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements;
    let Some(raw) = copy_window_info(options, kCGNullWindowID) else { return vec![] };
    let windows: CFArray<CFDictionary<CFString, CFType>> = unsafe {
        CFArray::wrap_under_get_rule(raw.as_concrete_TypeRef())
    };
    let pid_key = unsafe { CFString::wrap_under_get_rule(kCGWindowOwnerPID) };
    let mut seen = std::collections::HashSet::new();

    windows.iter().filter_map(|window| {
        let value = window.find(&pid_key)?;
        let pid = value.downcast::<CFNumber>()?.to_i64()? as u32;
        seen.insert(pid).then_some(pid)
    }).collect()
}
```

```rust
async fn discover_chrome_processes() -> Result<Vec<ChromeProcess>, NavigationError> {
    let mut command = Command::new("/bin/ps");
    command.args(["-axo", "pid=,command="]);
    let output = run_command(&mut command, "Chrome process discovery").await?;
    if !output.status.success() {
        return Err(NavigationError::new(
            NavigationErrorCode::TargetCommandFailed,
            "Could not inspect Chrome processes",
            Some(String::from_utf8_lossy(&output.stderr).trim().into()),
        ));
    }
    Ok(parse_chrome_processes(&String::from_utf8_lossy(&output.stdout)))
}
```

- [ ] **Step 5: Verify and commit**

Run: `cargo test --manifest-path src-tauri/Cargo.toml navigation`

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: all navigation tests pass and the crate checks on macOS.

Commit:

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/navigation.rs
git commit -m "feat: select the frontmost ordinary Chrome process"
```

### Task 3: PID-targeted exact tab lookup and ordinary-session fallback

**Files:**
- Modify: `src-tauri/src/navigation.rs`

**Interfaces:**
- Changes: `chrome_command(pid: u32, url: &str) -> Result<ScriptSpec, NavigationError>`.
- Consumes: PID from Task 2 and URL from existing Tauri command.
- Produces: a static AppleScriptObjC ScriptingBridge command with args `[pid, url]` plus a safe official-launcher fallback with args `[--new-tab, url]`.

- [ ] **Step 1: Write failing command-construction tests**

```rust
#[test]
fn chrome_pid_and_url_are_arguments_not_script_source() {
    let hostile = "https://example.com/\"%20&%20do%20shell%20script%20\"bad\"";
    let spec = chrome_command(712, hostile).unwrap();
    assert!(!spec.script.contains(hostile));
    assert_eq!(spec.args, vec!["712", hostile]);
    assert!(spec.script.contains("applicationWithProcessIdentifier"));
}

#[test]
fn chrome_script_reads_tab_urls_once_per_window() {
    let spec = chrome_command(712, "https://example.com").unwrap();
    assert!(spec.script.contains("valueForKey:\"URL\""));
    assert!(!spec.script.contains("URL of tab"));
}
```

- [ ] **Step 2: Run tests to verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml navigation::tests::chrome_`

Expected: tests fail because `chrome_command` still accepts only a URL and the script targets the bundle globally.

- [ ] **Step 3: Replace the Chrome script with static AppleScriptObjC**

The script must:

```applescript
use framework "Foundation"
use framework "AppKit"
use framework "ScriptingBridge"

on run argv
  set targetPid to (item 1 of argv) as integer
  set targetUrl to item 2 of argv
  set chromeApp to current application's SBApplication's applicationWithProcessIdentifier:targetPid
  set runningApp to current application's NSRunningApplication's runningApplicationWithProcessIdentifier:targetPid
  set chromeWindows to chromeApp's valueForKey:"windows"

  repeat with chromeWindow in chromeWindows
    set chromeTabs to chromeWindow's valueForKey:"tabs"
    set tabUrls to chromeTabs's valueForKey:"URL"
    repeat with tabIndex from 0 to ((count of tabUrls) - 1)
      if (tabUrls's objectAtIndex:tabIndex) as text is targetUrl then
        chromeWindow's setValue:(tabIndex + 1) forKey:"activeTabIndex"
        chromeWindow's setValue:1 forKey:"index"
        runningApp's activateWithOptions:3
        return "FOUND"
      end if
    end repeat
  end repeat

  return "NOT_FOUND"
end run
```

Keep the script as a Rust raw string constant. `chrome_command` validates the URL and returns `args: vec![pid.to_string(), url.to_string()]`. Add `chrome_open_args`, which validates the same URL and returns `vec!["--new-tab", url]`. Do not dynamically construct ScriptingBridge tabs: live testing showed that the proxy class cannot be initialized before it belongs to a container.

- [ ] **Step 4: Select PID before executing navigation**

Update `focus_chrome_url`:

```rust
let processes = discover_chrome_processes().await?;
let pid = select_chrome_pid(&processes, &chrome_window_order()).ok_or_else(|| {
    NavigationError::new(
        NavigationErrorCode::ChromeNotInstalled,
        "No ordinary Google Chrome process is running",
        Some("Remote-debugging Chrome processes are intentionally ignored".into()),
    )
})?;
let spec = chrome_command(pid, &url)?;
```

Invoke `/usr/bin/osascript -l AppleScript -e <static-script> -- <pid> <url>`. On `FOUND`, return success. On `NOT_FOUND`, invoke `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome --new-tab <url>` through `run_command`. Preserve existing permission classification and timeout handling.

- [ ] **Step 5: Verify and commit**

Run: `cargo test --manifest-path src-tauri/Cargo.toml navigation`

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: all navigation tests pass, including PID argument safety and timeout termination.

Commit:

```bash
git add src-tauri/src/navigation.rs
git commit -m "fix: route Chrome jumps to the ordinary instance"
```

### Task 4: Documentation, full verification, and live acceptance

**Files:**
- Modify: `README.md`

**Interfaces:**
- Documents: frontmost non-debug Chrome selection and debug-instance exclusion.
- Verifies: the existing settings test action and overlay 🌐 icon.

- [ ] **Step 1: Update user documentation**

Add to the Chrome section:

```markdown
当多个 Google Chrome 实例同时运行时，Focus Bar 会选择当前最前面的普通 Chrome，
并忽略带 `--remote-debugging-port` 的调试实例。若只有调试实例，请先启动普通 Chrome。
```

- [ ] **Step 2: Run full automated verification**

Run: `bun run check`

Run: `cargo test --manifest-path src-tauri/Cargo.toml`

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Run: `git diff --check`

Expected: 21 TypeScript tests pass, all non-ignored Rust tests pass, both frontend entries build, cargo check exits 0, and diff check is empty.

- [ ] **Step 3: Run live dual-instance acceptance**

With ordinary Chrome and the cmux remote-debugging Chrome both running:

1. Save a temporary exact localhost URL on the active Focus Bar task.
2. Ensure that URL already exists in ordinary Chrome and click `测试 Chrome`; confirm the ordinary window becomes frontmost and no new tab appears.
3. Close that tab, click `测试 Chrome` again, and confirm exactly one new tab opens in ordinary Chrome.
4. Confirm the debug Chrome process receives no matching tab.
5. Clear the temporary target, close the test tab, and confirm `~/.focus.json` contains no temporary navigation values.
6. Confirm no `osascript` process or macOS consent dialog remains.

- [ ] **Step 4: Commit documentation and integration**

```bash
git add README.md
git commit -m "docs: explain Chrome instance selection"
```

- [ ] **Step 5: Final repository audit**

Run: `git status --short --branch && git log --oneline -8`

Expected: branch is `main`; worktree is clean; the four task commits and prior design/plan commits are present.
