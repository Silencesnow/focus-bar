export type TaskStatus = "needs_action" | "needs_review" | "executing" | "idle";
export type LegacyTaskStatus = "blocked" | "review" | "verifying" | "done";
export type StoredTaskStatus = TaskStatus | LegacyTaskStatus;

export const STATUS_META: Record<TaskStatus, { color: string; emoji: string; label: string; bg: string }> = {
  needs_action: { color: "#ff453a", emoji: "🔴", label: "待处理", bg: "rgba(255,69,58,0.15)" },
  needs_review: { color: "#ff453a", emoji: "🔴", label: "待处理", bg: "rgba(255,69,58,0.15)" },
  executing:    { color: "#30d158", emoji: "🟢", label: "执行中",   bg: "rgba(48,209,88,0.15)" },
  idle:         { color: "#8e8e93", emoji: "⬜", label: "空闲",     bg: "rgba(142,142,147,0.08)" },
};

export interface VscodeTarget {
  workspace: string;
  workspace_name?: string;
  file?: string;
  line?: number;
}

export interface ChromeTarget {
  label?: string;
  url: string;
}

export type NavigationErrorCode =
  | "INVALID_TARGET"
  | "CHROME_NOT_INSTALLED"
  | "VSCODE_NOT_INSTALLED"
  | "AUTOMATION_PERMISSION_REQUIRED"
  | "ACCESSIBILITY_PERMISSION_REQUIRED"
  | "TARGET_COMMAND_FAILED"
  | "TARGET_TIMEOUT";

export interface NavigationError {
  code: NavigationErrorCode;
  message: string;
  detail?: string | null;
}

export interface TaskConfig {
  id: string;
  name: string;
  tab_icon?: string;
  name_overridden?: boolean;
  cmux_workspace_id?: string;
  cmux_directory?: string;
  codex_thread_id?: string;
  codex_directory?: string;
  manual_status?: StoredTaskStatus | null;
  manual_status_context_id?: string | null;
  last_viewed_at?: string | null;
  manually_collapsed_at?: string | null;
  note?: string;
  vscode?: VscodeTarget;
  chrome?: ChromeTarget | ChromeTarget[];
}

export interface FocusData {
  tasks: TaskConfig[];
  global_note?: string;
}

export interface CmuxWorkspace {
  id: string;
  ref: string;
  title: string;
  current_directory: string;
  listening_ports: number[];
  latest_conversation_message: string | null;
  latest_submitted_at: string | null;
  active_surface_title?: string | null;
  active_surface_progress?: string | null;
  agent_lifecycle?: "running" | "idle" | "needsInput" | "unknown" | null;
  background_shell_process?: string | null;
  agent_event_kind?: "question" | "stop" | "running" | null;
  agent_event_at?: string | null;
  selected: boolean;
  index: number;
  window_id: string;
}

export interface CmuxNotification {
  id: string;
  workspace_id: string;
  title: string;
  subtitle: string;
  body: string;
  is_read: boolean;
  created_at: string;
  tab_title: string | null;
}

export type CmuxSourceErrorCode =
  | "CLI_NOT_FOUND"
  | "CMUX_NOT_RUNNING"
  | "ACCESS_DENIED"
  | "TIMEOUT"
  | "INVALID_RESPONSE"
  | "WATCHER_DISCONNECTED";

export type CmuxSourceState =
  | { status: "ready"; cli_path: string; socket_path: string | null }
  | { status: "error"; code: CmuxSourceErrorCode; message: string; detail: string | null };

export interface CmuxSnapshot {
  source: CmuxSourceState;
  workspaces: CmuxWorkspace[];
  notifications: CmuxNotification[];
  fetched_at: number;
}

export type CodexLifecycle = "executing" | "needs_input" | "completed" | "failed" | "idle";

export interface CodexThread {
  id: string;
  title: string;
  cwd: string;
  lifecycle: CodexLifecycle;
  updated_at: number;
  activity_at: number | null;
  latest_message: string | null;
}

export type CodexSourceState =
  | { status: "ready"; state_path: string }
  | { status: "error"; message: string; detail: string | null };

export interface CodexSnapshot {
  source: CodexSourceState;
  threads: CodexThread[];
  fetched_at: number;
}

export interface MergedTask {
  config: TaskConfig;
  source?: "cmux" | "codex";
  cmux?: CmuxWorkspace;
  codex?: CodexThread;
  notifications: CmuxNotification[];
  hasUnread: boolean;
  latestNotifSubtitle: string | null;
  effectiveStatus: TaskStatus;
  ports: number[];
  directory: string;
  title: string;
  latestMessage: string | null;
  statusReason: string | null;
  activitySummary: string | null;
  activityAt: string | null;
}
