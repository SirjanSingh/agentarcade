#!/usr/bin/env node
/**
 * Cursor → claudegame bridge. Install to ~/.cursor/hooks/ and wire in
 * ~/.cursor/hooks.json — see README "Cursor hooks".
 *
 * Usage (called by Cursor, not by hand):
 *   node claudegame.js UserPromptSubmit
 *   node claudegame.js Stop
 *   node claudegame.js PreCompact
 */
const PORT = process.env.CLAUDEGAME_PORT || "47913";
const EVENT = process.argv[2];

if (!EVENT) process.exit(0);

const chunks = [];
process.stdin.on("data", (d) => chunks.push(d));
process.stdin.on("end", () => {
  let sessionId = "cursor";
  try {
    const raw = Buffer.concat(chunks).toString("utf8").trim();
    if (raw) {
      const j = JSON.parse(raw);
      sessionId =
        j.conversation_id ||
        j.conversationId ||
        j.session_id ||
        j.sessionId ||
        j.chat_id ||
        j.chatId ||
        sessionId;
    }
  } catch {
    // stdin optional
  }

  fetch(`http://127.0.0.1:${PORT}/event`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ hook_event_name: EVENT, session_id: sessionId }),
  }).catch(() => {});
});
