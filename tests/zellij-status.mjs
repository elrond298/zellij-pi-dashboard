import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

const runtime = await mkdtemp(join(tmpdir(), "pi-zellij-status-test-"));
process.env.XDG_RUNTIME_DIR = runtime;
process.env.ZELLIJ_SESSION_NAME = "goal-test";

const { default: extension } = await import("../extensions/zellij-status.ts");
const handlers = new Map();
const entries = [];
let branch = [];
let deferred;
const completions = [];
const completionText = "优化👨‍👩‍👧‍👦自适应状态显示".repeat(8);
const pi = {
  on(event, handler) {
    handlers.set(event, handler);
  },
  getSessionName() {
    return undefined;
  },
  appendEntry(customType, data) {
    const entry = { type: "custom", customType, data };
    entries.push(entry);
    branch.push(entry);
  },
  async exec() {
    return { code: 1, stdout: "", stderr: "" };
  },
};
const ctx = {
  cwd: process.cwd(),
  model: { id: "test" },
  thinkingLevel: "off",
  isIdle: () => false,
  getContextUsage: () => undefined,
  sessionManager: {
    getEntries: () => [],
    getBranch: () => branch,
    getSessionId: () => "session-id",
  },
  modelRegistry: {
    find: () => ({}),
    async complete(_model, request, options) {
      completions.push({ request, options });
      if (deferred) return deferred;
      return { stopReason: "stop", content: [{ type: "text", text: completionText }] };
    },
  },
};
const statusFile = join(
  runtime,
  `pi-zellij-status-${process.getuid()}`,
  "session-goal-test",
  `${process.pid}.json`,
);
const waitFor = async (predicate) => {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    try {
      const status = JSON.parse(await readFile(statusFile, "utf8"));
      if (predicate(status)) return status;
    } catch {}
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
  assert.fail("timed out waiting for status JSON");
};
const waitForCompletions = async (count) => {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (completions.length >= count) return;
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
  assert.fail(`timed out waiting for ${count} completion calls`);
};
const family = "👨‍👩‍👧‍👦";
const statusWidth = (text) =>
  Array.from(new Intl.Segmenter(undefined, { granularity: "grapheme" }).segment(text)).reduce(
    (width, { segment }) =>
      width + (segment === family || /\p{Script=Han}/u.test(segment) ? 2 : 1),
    0,
  );

try {
  extension(pi);
  await handlers.get("session_start")({}, ctx);

  const goal = (
    "Refactor adaptive status rendering without hiding stable blocks " + "detail ".repeat(800)
  ).trim();
  const expectedGoalKey = createHash("sha256").update(goal).digest("base64url");
  handlers.get("message_start")(
    { message: { content: `<goal_objective>${goal}</goal_objective>` } },
    ctx,
  );
  const summarized = await waitFor(
    (status) => status.goal?.startsWith("优化") && status.goal.endsWith("…"),
  );
  const summary = summarized.goal;
  assert.ok(statusWidth(summary) <= 48, summary);
  assert.ok(!/[👨👩👧👦‍]/u.test(summary.replaceAll(family, "")), summary);
  assert.equal(completions.length, 1);
  assert.equal(completions[0].request.messages.length, 1);
  assert.equal(
    completions[0].request.messages[0].content[0].text,
    `Input text:\n---\n${goal.slice(0, 2000)}\n…\n${goal.slice(-2000)}\n---`,
  );
  assert.equal(completions[0].options.toolChoice, "none");
  assert.equal(completions[0].options.maxRetries, 0);
  assert.equal(completions[0].options.timeoutMs, 15_000);

  const summaryEntry = entries.find((entry) => entry.customType === "zellij-goal-summary");
  assert.deepEqual(summaryEntry.data, { goalKey: expectedGoalKey, summary });
  branch = [
    {
      type: "custom",
      customType: "goal-state",
      data: { goal: { id: "goal-1", text: goal, status: "active" } },
    },
    summaryEntry,
  ];
  handlers.get("before_agent_start")({}, ctx);
  const restored = await waitFor(
    (status) => status.goal === summary && status.goalDetail?.text === goal,
  );
  assert.equal(restored.goalDetail.text, goal);
  assert.equal(completions.length, 1, "restored summary must not be regenerated");

  const shortGoal = "修复状态";
  handlers.get("message_start")(
    { message: { content: `<goal_objective>${shortGoal}</goal_objective>` } },
    ctx,
  );
  await waitFor((status) => status.goal === shortGoal);
  await new Promise((resolve) => setTimeout(resolve, 10));
  assert.equal(completions.length, 1, "short goals must stay verbatim without a model call");

  let resolveDeferred;
  deferred = new Promise((resolve) => {
    resolveDeferred = resolve;
  });
  const replacement =
    "Replace the active goal before its summary returns " + "without blocking Pi ".repeat(8);
  handlers.get("message_start")(
    { message: { content: `<goal_objective>${replacement}</goal_objective>` } },
    ctx,
  );
  await waitForCompletions(2);
  const restoredReplacement =
    "Restore a different active goal and summarize its current objective ".repeat(4);
  branch = [
    {
      type: "custom",
      customType: "goal-state",
      data: { goal: { id: "goal-2", text: restoredReplacement, status: "active" } },
    },
  ];
  handlers.get("before_agent_start")({}, ctx);
  await waitForCompletions(3);
  assert.equal(completions[1].options.signal.aborted, true, "restore must cancel the stale goal");
  handlers.get("tool_execution_end")({
    toolCallId: "goal-complete",
    toolName: "goal_complete",
    isError: false,
    result: {},
  });
  resolveDeferred({ stopReason: "stop", content: [{ type: "text", text: "stale summary" }] });
  const completed = await waitFor((status) => status.goal === undefined);
  assert.equal(completed.goal, undefined);
  assert.equal(entries.filter((entry) => entry.customType === "zellij-goal-summary").length, 1);

  const shutdownGoal = "Summarize this goal only if the extension is still running ".repeat(4);
  handlers.get("message_start")(
    { message: { content: `<goal_objective>${shutdownGoal}</goal_objective>` } },
    ctx,
  );
  await handlers.get("session_shutdown")();
  await new Promise((resolve) => setTimeout(resolve, 10));
  assert.equal(completions.length, 3, "shutdown must cancel a queued summary request");
} finally {
  await handlers.get("session_shutdown")();
  await rm(runtime, { recursive: true, force: true });
}

console.log("zellij status goal summary: ok");
