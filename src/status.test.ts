import { describe, expect, test } from "bun:test";
import { deriveTaskStatus, normalizeManualStatus, statusReason } from "./status";
import type { CmuxNotification } from "./types";

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

describe("deriveTaskStatus", () => {
  test("needs action outranks review even when the review is newer", () => {
    expect(deriveTaskStatus({
      manualStatus: null,
      latestSubmittedAt: null,
      notifications: [
        notification({ id: "done", subtitle: "Completed" }),
        notification({ id: "wait", subtitle: "Waiting for input", created_at: "2026-07-10T09:00:00Z" }),
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
