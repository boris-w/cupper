//! The compat suite's scenarios (design 5.9.5): a prompt, what the user does
//! after it, and structural checks on the host's events, never on the
//! agent's words. The prompt is the fake agent's script.

pub struct Scenario {
    pub id: &'static str,
    pub title: &'static str,
    /// Its fake recording is a GOLD-B sample (`xtask compat --fake --write`).
    pub gold_b: bool,
    /// The permission mode the session starts in, when not the agent's default.
    pub mode: Option<&'static str>,
    pub script: &'static str,
    pub steps: &'static [Step],
    pub checks: &'static [Check],
}

/// What the user does after `session/create`, in order.
pub enum Step {
    /// Waits for the next request without answering it.
    Asked,
    /// Answers the next request with its option of this kind (only
    /// `allow_once` and `reject_once`, design 5.9.5).
    Choose(&'static str),
    /// Answers the next question: each field's first choice, or this text.
    Answer(&'static str),
    Pause(u64),
    Interrupt,
}

pub enum Check {
    /// The turn ended with this status.
    Turn(&'static str),
    /// An item of this kind.
    Item(&'static str),
    /// A tool call of this kind.
    Tool(&'static str),
    /// A tool call that ended with an exit code other than 0.
    ExitCode,
    /// A request of this type ended with this status.
    Request(&'static str, &'static str),
    /// The session learnt the agent's options.
    Capabilities,
    /// The agent left no file at this path in the project.
    NoFile(&'static str),
}

const DONE: Check = Check::Turn("completed");

pub const SCENARIOS: &[Scenario] = &[
    Scenario {
        id: "new-session",
        title: "a new session and its capabilities",
        script: "say ready",
        checks: &[Check::Capabilities, Check::Item("agent_message"), DONE],
        ..BASE
    },
    Scenario {
        id: "normal-turn",
        title: "a turn with a thought and a tool call",
        gold_b: true,
        script: "say hello; think pondering; tool t1 read Read notes; tool_done t1 20; say done",
        checks: &[Check::Tool("read"), Check::Item("agent_message"), DONE],
        ..BASE
    },
    Scenario {
        id: "command-exit",
        title: "a command that exits with a non-zero code",
        script: "permission t1 execute; terminal t1 2 1; say done",
        steps: &[Step::Choose("allow_once")],
        checks: &[Check::Tool("execute"), Check::ExitCode, DONE],
        ..BASE
    },
    Scenario {
        id: "approval",
        title: "a command allowed once",
        gold_b: true,
        script: "tool t1 execute Run tests; permission t1; say finished",
        steps: &[Step::Choose("allow_once")],
        checks: &[Check::Request("permission", "resolved"), DONE],
        ..BASE
    },
    Scenario {
        id: "reject",
        title: "a command rejected once",
        script: "tool t1 execute Run tests; permission t1; say finished",
        steps: &[Step::Choose("reject_once")],
        checks: &[
            Check::Request("permission", "resolved"),
            Check::NoFile("rejected.txt"),
            DONE,
        ],
        ..BASE
    },
    Scenario {
        id: "question",
        title: "a structured question answered",
        script: "question colour?",
        steps: &[Step::Answer("blue")],
        checks: &[Check::Request("question", "resolved"), DONE],
        ..BASE
    },
    Scenario {
        id: "plan",
        title: "a plan, sent back to planning",
        mode: Some("plan"),
        script: "plan_update # Plan\\n- one step; plan_confirm; say after",
        steps: &[Step::Choose("reject_once")],
        checks: &[
            Check::Item("plan"),
            Check::Request("plan_confirm", "resolved"),
            Check::NoFile("hello.txt"),
            DONE,
        ],
        ..BASE
    },
    Scenario {
        id: "cancel",
        title: "an interrupt while a command runs",
        gold_b: true,
        script: "tool t1 execute Run sleep; permission t1; wait_cancel",
        steps: &[
            Step::Choose("allow_once"),
            Step::Pause(1000),
            Step::Interrupt,
        ],
        checks: &[Check::Turn("cancelled")],
        ..BASE
    },
    Scenario {
        id: "cancel-approval",
        title: "an interrupt while a request waits",
        script: "tool t1 execute Run tests; permission t1; say finished",
        steps: &[Step::Asked, Step::Interrupt],
        checks: &[
            Check::Request("permission", "cancelled"),
            Check::Turn("cancelled"),
        ],
        ..BASE
    },
    Scenario {
        id: "thinking",
        title: "a thought stream",
        script: "think pondering; say done",
        checks: &[Check::Item("thought"), DONE],
        ..BASE
    },
];

const BASE: Scenario = Scenario {
    id: "",
    title: "",
    gold_b: false,
    mode: None,
    script: "",
    steps: &[],
    checks: &[],
};

impl Check {
    /// How the report names it.
    pub fn name(&self) -> String {
        match self {
            Self::Turn(status) => format!("the turn ended {status}"),
            Self::Item(kind) => format!("an item of kind {kind}"),
            Self::Tool(kind) => format!("a tool call of kind {kind}"),
            Self::ExitCode => "a tool call with a non-zero exit code".into(),
            Self::Request(kind, status) => format!("a {kind} request {status}"),
            Self::Capabilities => "the session's capabilities".into(),
            Self::NoFile(path) => format!("no file {path}"),
        }
    }
}
