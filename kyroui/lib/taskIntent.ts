/**
 * Explicit intent classification for client-side plan suggestions (Bug 5).
 *
 * The previous implementation used a substring regex (`/test/i`) that matched
 * inside unrelated words such as "latest". Classification is word-boundary
 * aware here, and the kind of task drives only a *suggested workflow* — it
 * never claims a backend execution plan; the engine's own plan (when present)
 * is backend-authoritative.
 */

export interface TaskIntent {
  isConsequential: boolean;
  kinds: {
    organize: boolean;
    email: boolean;
    development: boolean;
    schedule: boolean;
  };
}

/** `\b`-bounded so "latest" never matches /test/, "remove" never matches /move/. */
function matchesAnyWord(prompt: string, words: string[]): boolean {
  const lower = prompt.toLowerCase();
  return words.some((w) =>
    new RegExp(`(^|[^a-z0-9_])${w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}([^a-z0-9_]|$)`, "i").test(
      lower
    )
  );
}

const ORGANIZE_WORDS = [
  "organize", "organise", "sort", "tidy", "deduplicate", "cleanup", "clean up",
  "clean", "delete", "remove", "move the", "move all", "move my", "move every",
  "rearrange", "into folders",
];

const EMAIL_WORDS = [
  "email", "inbox", "reply", "draft a", "send a", "send the", "mail to", "compose",
];

const DEVELOPMENT_WORDS = [
  "build", "test", "tests", "pytest", "cargo", "npm", "vitest", "jest", "compile",
  "lint", "commit", "push", "merge", "deploy", "deploying",
];

const SCHEDULE_WORDS = [
  "schedule", "routine", "calendar", "remind", "appointment", "cron",
];

export function classifyTaskIntent(prompt: string): TaskIntent {
  const text = prompt || "";
  const kinds = {
    organize: matchesAnyWord(text, ORGANIZE_WORDS),
    email: matchesAnyWord(text, EMAIL_WORDS),
    development: matchesAnyWord(text, DEVELOPMENT_WORDS),
    schedule: matchesAnyWord(text, SCHEDULE_WORDS),
  };
  const isConsequential = kinds.organize || kinds.email || kinds.development || kinds.schedule;

  return { isConsequential, kinds };
}

/**
 * Honest resource claim set for a suggested workflow: every entry names a
 * mechanism the engine will actually engage for this class of task, instead
 * of generic labels presented without justification.
 */
export function suggestedResourcesForIntent(
  intent: TaskIntent,
  agentName?: string | null
): string[] {
  const resources: string[] = [
    agentName ? `@${agentName}` : "Kyro agent",
    "SQLite audit ledger (run events)",
  ];
  if (intent.kinds.organize) {
    resources.push("Authorized folders (paths outside them need your approval)");
  }
  if (intent.kinds.development) {
    resources.push("Sandboxed project commands (repository-controlled code)");
  }
  if (intent.kinds.email) {
    resources.push("Approval-gated connectors (email needs your confirmation)");
  }
  if (intent.kinds.schedule) {
    resources.push("Routine scheduler (persisted to local database)");
  }
  return resources;
}
