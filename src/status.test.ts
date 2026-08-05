import { describe, expect, test } from "bun:test";
import { deriveTaskStatus, normalizeManualStatus, statusReason } from "./status";
import { STATUS_META, type CmuxNotification } from "./types";

function notification(overrides: Partial<CmuxNotification> = {}): CmuxNotification {
  return {
    id: "n1",
    workspace_id: "w1",
    title: "Agent",
    subtitle: "",
    body: "",
    is_read: false,
    created_at: "2026-07-10T10:00:00Z",
    tab_title: null,
    ...overrides,
  };
}

test("action and review share one red pending presentation", () => {
  expect(STATUS_META.needs_action).toEqual(STATUS_META.needs_review);
  expect(STATUS_META.needs_review.label).toBe("待处理");
  expect(STATUS_META.needs_review.emoji).toBe("🔴");
  expect(STATUS_META.needs_review.color).toBe("#ff453a");
});

describe("deriveTaskStatus", () => {
  test("needs action outranks review even when the review is newer", () => {
    expect(deriveTaskStatus({
      manualStatus: null,
      latestSubmittedAt: null,
      notifications: [
        notification({ id: "done", subtitle: "Completed" }),
        notification({ id: "wait", subtitle: "Input required", created_at: "2026-07-10T09:00:00Z" }),
      ],
    })).toBe("needs_action");
  });

  test("completed unread work needs review", () => {
    expect(deriveTaskStatus({
      manualStatus: null,
      latestSubmittedAt: null,
      notifications: [notification({ body: "DONE successfully" })],
    })).toBe("needs_review");
  });

  test("read terminal notification becomes idle", () => {
    expect(deriveTaskStatus({
      manualStatus: null,
      latestSubmittedAt: null,
      notifications: [notification({ subtitle: "Waiting", is_read: true })],
    })).toBe("idle");
  });

  test("new submission after a terminal notification is executing", () => {
    expect(deriveTaskStatus({
      manualStatus: null,
      latestSubmittedAt: "2026-07-10T11:00:00Z",
      notifications: [notification({ subtitle: "Completed", is_read: true })],
    })).toBe("executing");
  });

  test("a viewed submission without a live running signal becomes idle", () => {
    expect(deriveTaskStatus({
      manualStatus: null,
      latestSubmittedAt: "2026-07-29T07:47:35Z",
      activeSurfaceTitle: "~/Documents/work/ling-design-C",
      lastViewedAt: "2026-07-29T08:43:01Z",
      notifications: [],
    })).toBe("idle");
  });

  test("a spinning selected surface is executing when cmux omits submission metadata", () => {
    const input = {
      manualStatus: null,
      latestSubmittedAt: null,
      activeSurfaceTitle: "⠂ 支持yarn serve命令动态配置端口",
      notifications: [notification({ subtitle: "Waiting", is_read: true })],
    } as Parameters<typeof deriveTaskStatus>[0];

    expect(deriveTaskStatus(input)).toBe("executing");
  });

  test("a running background subagent outranks a stale unread waiting notification", () => {
    const input = {
      manualStatus: null,
      latestSubmittedAt: null,
      activeSurfaceTitle: "⠂ 优化imageManager初始化加载机制",
      notifications: [notification({ body: "Claude is waiting for your input" })],
    } as Parameters<typeof deriveTaskStatus>[0];

    expect(deriveTaskStatus(input)).toBe("executing");
    expect(statusReason(input)).toBeNull();
  });

  test("legacy needsInput lifecycle alone means an unseen stop, not an explicit question", () => {
    expect(deriveTaskStatus({
      manualStatus: null,
      latestSubmittedAt: null,
      agentLifecycle: "needsInput",
      notifications: [],
    })).toBe("needs_review");
  });

  test("cmux running lifecycle remains executing without submission metadata", () => {
    expect(deriveTaskStatus({
      manualStatus: null,
      latestSubmittedAt: null,
      agentLifecycle: "running",
      notifications: [],
    })).toBe("executing");
  });

  test("a newly stopped Claude surface needs review before its delayed notification", () => {
    expect(deriveTaskStatus({
      manualStatus: null,
      latestSubmittedAt: null,
      activeSurfaceTitle: "✳ 支持yarn serve命令动态配置端口",
      notifications: [notification({ subtitle: "Waiting", is_read: true })],
    })).toBe("needs_review");
  });

  test("a stopped Claude surface becomes idle after it is viewed", () => {
    expect(deriveTaskStatus({
      manualStatus: null,
      latestSubmittedAt: null,
      activeSurfaceTitle: "✳ 支持yarn serve命令动态配置端口",
      lastViewedAt: "2026-07-14T10:00:00Z",
      notifications: [],
    })).toBe("idle");
  });

  test("an explicit agent question needs action", () => {
    expect(deriveTaskStatus({
      manualStatus: null,
      latestSubmittedAt: null,
      agentEventKind: "question",
      agentEventAt: "2026-07-14T10:00:00Z",
      notifications: [],
    })).toBe("needs_action");
  });

  test("a normal stop after the last view needs review", () => {
    expect(deriveTaskStatus({
      manualStatus: null,
      latestSubmittedAt: null,
      agentEventKind: "stop",
      agentEventAt: "2026-07-14T10:00:00Z",
      lastViewedAt: "2026-07-14T09:00:00Z",
      notifications: [],
    })).toBe("needs_review");
  });

  test("a normal stop becomes idle when it has been viewed", () => {
    expect(deriveTaskStatus({
      manualStatus: null,
      latestSubmittedAt: null,
      agentEventKind: "stop",
      agentEventAt: "2026-07-14T10:00:00Z",
      lastViewedAt: "2026-07-14T11:00:00Z",
      notifications: [],
    })).toBe("idle");
  });

  test("the generic Claude waiting notification means review, not action", () => {
    expect(deriveTaskStatus({
      manualStatus: null,
      latestSubmittedAt: null,
      notifications: [notification({ body: "Claude is waiting for your input" })],
    })).toBe("needs_review");
  });

  test("a Claude background shell keeps the task executing after the response ends", () => {
    expect(deriveTaskStatus({
      manualStatus: null,
      latestSubmittedAt: null,
      activeSurfaceTitle: "✳ 编译任务",
      backgroundShellProcess: "ninja",
      notifications: [notification({ subtitle: "Completed" })],
    })).toBe("executing");
  });

  test("a terminal notification after the latest submission is idle once read", () => {
    expect(deriveTaskStatus({
      manualStatus: null,
      latestSubmittedAt: "2026-07-10T09:00:00Z",
      notifications: [notification({ subtitle: "Completed", is_read: true })],
    })).toBe("idle");
  });

  test("manual needs action is respected", () => {
    expect(deriveTaskStatus({
      manualStatus: "needs_action",
      latestSubmittedAt: null,
      notifications: [],
    })).toBe("needs_action");
  });
});

test("legacy manual statuses migrate", () => {
  expect(normalizeManualStatus("blocked")).toBe("needs_action");
  expect(normalizeManualStatus("review")).toBe("needs_review");
  expect(normalizeManualStatus("verifying")).toBe("needs_review");
  expect(normalizeManualStatus("done")).toBe("idle");
  expect(normalizeManualStatus(null)).toBeNull();
});

test("status reason uses the newest matching notification", () => {
  expect(statusReason({
    manualStatus: null,
    latestSubmittedAt: null,
    notifications: [
      notification({ id: "old", subtitle: "Waiting", body: "old reason", created_at: "2026-07-10T09:00:00Z" }),
      notification({ id: "new", subtitle: "Blocked", body: "new reason", created_at: "2026-07-10T11:00:00Z" }),
    ],
  })).toContain("new reason");
});
