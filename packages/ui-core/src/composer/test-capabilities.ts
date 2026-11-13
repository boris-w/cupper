// Test data: a `session.capabilities` event the views accept, for the
// composer's and the request panel's tests.

/** An agent with images, no commands and no options, that cannot steer, but `more`. */
export const capabilities = (more: object) => ({
  type: "session.capabilities",
  data: {
    ...{ agent: "claude", commands: [], config: [] },
    prompt: { image: true, file_ref: true, max_image_bytes: 1024 * 1024 },
    session: { resume: true, load_history: false, fork: false },
    turn: {
      ...{ queue: true, steer: { supported: false, side_effects: false } },
      ...{ interrupt: true, interrupt_stops_commands: true },
    },
    interactions: { reject_ends_turn: false, question_forms: true, plan_confirm: true },
    extraction: {
      ...{ exit_code: true, output_streaming: false, diff: "per_file", thinking: true },
      ...{ plan: "markdown_only", run_state: "inferred" },
    },
    versions: { adapter: "1", cli: "1", cli_compat: "verified" },
    ...more,
  },
});

/** A running turn's `turn` capabilities, steering as given. */
export const steering = (supported: boolean, side_effects = false) => ({
  ...{ queue: true, steer: { supported, side_effects } },
  ...{ interrupt: true, interrupt_stops_commands: true },
});

/** Turn `t_{n}` started and still running. */
export const running = (n = 1) => ({
  type: "turn.started",
  data: { turn_id: `t_${n}`, turn_no: n, started_at: n, origin: "user" },
});
