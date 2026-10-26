//! The compat suite's scenarios (design 5.9.5): a prompt, what the user does
//! after it, and structural checks on the host's events, never on the
//! agent's words. The real CLI gets the prompt, the fake agent the script.
//! `id`s never change: T188 and the COMPAT IDs name scenarios by them.

pub struct Scenario {
    pub id: &'static str,
    pub title: &'static str,
    /// Its fake recording is a GOLD-B sample (`xtask compat --fake --write`).
    pub gold_b: bool,
    /// The permission mode the session starts in, always given.
    pub mode: &'static str,
    /// It may loosen the tier or pick a privileged option, in its temporary
    /// repository only (the self-constraints, design 5.9.5).
    pub loosens: bool,
    /// The model a real run uses instead of the run's.
    pub model: Option<&'static str>,
    /// In the temporary repository before the session starts.
    pub files: &'static [(&'static str, &'static str)],
    /// Why it cannot run on this version.
    pub skip: Option<&'static str>,
    /// Why the fake agent cannot play it: it reads the CLI's configuration.
    pub real_only: Option<&'static str>,
    /// The project's directory in the repository, when not its root.
    pub project: Option<&'static str>,
    /// The project is added without trusting its configuration.
    pub untrusted: bool,
    pub prompt: &'static str,
    pub script: &'static str,
    pub steps: &'static [Step],
    pub checks: &'static [Check],
}

/// The permission modes a session may start in without `loosens`.
const MODES: &[(&str, &[&str])] = &[
    ("claude", &["default", "plan"]),
    ("codex", &["read-only", "workspace-write"]),
];

impl Scenario {
    /// What in it the self-constraints refuse, for `agent`.
    pub fn refused(&self, agent: &str) -> Option<String> {
        let modes = MODES.iter().find(|(name, _)| *name == agent);
        // A declared loosening still names its mode.
        let allowed = modes.is_some_and(|(_, modes)| modes.contains(&self.mode));
        if !allowed && (!self.loosens || self.mode.is_empty()) {
            return Some(format!("{agent} in permission mode `{}`", self.mode));
        }
        self.steps.iter().find_map(|step| match step {
            Step::Choose(kind) if !["allow_once", "reject_once"].contains(kind) => {
                Some(format!("an option of kind {kind}"))
            }
            _ => None,
        })
    }
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
    /// Sends the next prompt once the turn before has ended.
    Send {
        real: &'static str,
        fake: &'static str,
    },
    /// The same with an image uploaded first.
    Image {
        real: &'static str,
        fake: &'static str,
    },
    /// Kills the real CLI (`claude`) under the adapter; the fake agent's
    /// script crashes it instead.
    KillCli,
    /// Stops the host and starts it again on the same data.
    RestartHost,
    /// Kills the CLI and sends once more, this many times, counting the
    /// host's processes each time (Q43).
    Resumes(usize),
    /// Sessions on the same adapter, each with prompts sent at once, in
    /// rounds; each prompt asks for a marker of its own (Q8).
    Concurrent {
        sessions: usize,
        prompts: usize,
        rounds: usize,
    },
    /// Writes a file in the repository.
    WriteFile(&'static str, &'static str),
    /// Ends the scenario without waiting for the turn.
    Leave,
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
    /// A file of the repository that does not hold this text.
    Lacks(&'static str, &'static str),
    /// A notice of this code.
    Notice(&'static str),
    /// Each session's answers hold its own markers and no other's.
    NoCrossTalk,
    /// The host's processes did not grow over the resumes.
    Steady,
    /// A command ran below the adapter when the scenario ended.
    Running,
    /// The host started the session with the user's settings only.
    UserSettings,
    /// The host passed the prompt's image on to the agent.
    ImageSent,
    /// The host sent the agent this method.
    Sent(&'static str),
    /// Whether this file of the repository exists, reported, not judged.
    Observe(&'static str),
}

const DONE: Check = Check::Turn("completed");

pub const SCENARIOS: &[Scenario] = &[
    Scenario {
        id: "new-session",
        title: "a new session and its capabilities",
        mode: "default",
        prompt: "Reply with the single word: ready",
        script: "say ready",
        checks: &[Check::Capabilities, Check::Item("agent_message"), DONE],
        ..BASE
    },
    Scenario {
        id: "normal-turn",
        title: "a turn with a thought and a tool call",
        gold_b: true,
        mode: "default",
        files: &[("notes.txt", "The compat suite reads this file.\n")],
        prompt: "Read the file notes.txt and summarize it in one sentence.",
        script: "say hello; think pondering; tool t1 read Read notes; tool_done t1 20; say done",
        checks: &[Check::Tool("read"), Check::Item("agent_message"), DONE],
        ..BASE
    },
    Scenario {
        id: "command-exit",
        title: "a command that exits with a non-zero code",
        mode: "default",
        prompt: "Run the shell command `ls no-such-file` and tell me its exit code.",
        script: "permission t1 execute; terminal t1 2 1; say done",
        steps: &[Step::Choose("allow_once")],
        checks: &[Check::Tool("execute"), Check::ExitCode, DONE],
        ..BASE
    },
    Scenario {
        id: "approval",
        title: "a command allowed once",
        gold_b: true,
        mode: "default",
        prompt: "Run the shell command `echo cupper` and tell me what it printed.",
        script: "tool t1 execute Run tests; permission t1; say finished",
        steps: &[Step::Choose("allow_once")],
        checks: &[Check::Request("permission", "resolved"), DONE],
        ..BASE
    },
    Scenario {
        id: "reject",
        title: "a command rejected once",
        mode: "default",
        prompt: "Run the shell command `touch rejected.txt`.",
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
        mode: "default",
        prompt: "Use your tool for asking me questions to ask whether I prefer red or blue, \
                 then tell me my answer.",
        script: "question colour?",
        steps: &[Step::Answer("blue")],
        checks: &[Check::Request("question", "resolved"), DONE],
        ..BASE
    },
    Scenario {
        id: "plan",
        title: "a plan, sent back to planning",
        mode: "plan",
        prompt: "Plan how to add a file hello.txt that says hello. Do not change anything.",
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
        mode: "default",
        prompt: "Run the shell command `sleep 60`, then say done.",
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
        mode: "default",
        prompt: "Run the shell command `echo cupper` and tell me what it printed.",
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
        mode: "default",
        // COMPAT-claude-thinking: haiku does not think aloud.
        model: Some("sonnet"),
        prompt: "Think it through step by step, then tell me: what is 17 times 23?",
        script: "think pondering; say done",
        checks: &[Check::Item("thought"), DONE],
        ..BASE
    },
    Scenario {
        id: "steer",
        title: "a message steered into the running turn",
        mode: "default",
        skip: Some("Steer comes with M1b: in M1a `session/send` in steer mode queues the message"),
        ..BASE
    },
    Scenario {
        id: "crash-resume",
        title: "the CLI killed, then the session resumed",
        mode: "default",
        prompt: "Reply with the single word: one",
        script: "say one",
        steps: &[
            Step::KillCli,
            Step::Send {
                real: "Reply with the single word: two",
                fake: "crash_cli",
            },
            Step::Send {
                real: "Reply with the single word: three",
                fake: "whoami",
            },
        ],
        checks: &[Check::Notice("session_resumed"), DONE],
        ..BASE
    },
    Scenario {
        id: "host-restart",
        title: "the host restarted, then the session resumed",
        mode: "default",
        prompt: "Reply with the single word: one",
        script: "say one",
        steps: &[
            Step::RestartHost,
            Step::Send {
                real: "Reply with the single word: two",
                fake: "whoami",
            },
        ],
        checks: &[Check::Sent("session/resume"), DONE],
        ..BASE
    },
    Scenario {
        id: "image",
        title: "a prompt with an image",
        mode: "default",
        prompt: "Reply with the single word: ready",
        script: "say ready",
        steps: &[Step::Image {
            real: "What colour is the attached image? Reply with one word.",
            fake: "say seen",
        }],
        checks: &[Check::ImageSent, DONE],
        ..BASE
    },
    Scenario {
        id: "concurrent-two",
        title: "two sessions prompting at once on one adapter (Q8)",
        mode: "default",
        prompt: "Reply with the single word: ready",
        script: "say ready",
        steps: &[Step::Concurrent {
            sessions: 2,
            prompts: 3,
            rounds: 3,
        }],
        checks: &[Check::NoCrossTalk, DONE],
        ..BASE
    },
    Scenario {
        id: "six-sessions",
        title: "six sessions at once on one adapter",
        mode: "default",
        prompt: "Reply with the single word: ready",
        script: "say ready",
        steps: &[Step::Concurrent {
            sessions: 6,
            prompts: 1,
            rounds: 1,
        }],
        checks: &[Check::NoCrossTalk, DONE],
        ..BASE
    },
    Scenario {
        id: "close-running",
        title: "the adapter closed while a command runs (K21)",
        mode: "default",
        prompt: "Run the shell command `sleep 300`.",
        // A command in a process group of its own, as Claude's are.
        script: "tool t1 execute Run sleep; permission t1; \
            write_file run.sh exec /usr/bin/perl -e 'setpgrp(0,0), exec(\"/bin/sleep\", \"300\")' \
            </dev/null >/dev/null 2>&1; spawn /bin/sh run.sh; wait_cancel",
        steps: &[Step::Choose("allow_once"), Step::Pause(3000), Step::Leave],
        checks: &[Check::Running],
        ..BASE
    },
    Scenario {
        id: "resume-procs",
        title: "the same session resumed again and again (Q43)",
        mode: "default",
        prompt: "Reply with the single word: one",
        script: "say one",
        steps: &[Step::Resumes(5)],
        checks: &[Check::Steady, DONE],
        ..BASE
    },
    Scenario {
        id: "untrusted-project",
        title: "an untrusted project's configuration takes no effect",
        mode: "default",
        untrusted: true,
        files: &[
            (".claude/settings.json", UNTRUSTED_SETTINGS),
            (
                ".mcp.json",
                r#"{"mcpServers": {"compat": {"command": "/usr/bin/touch", "args": ["mcp-ran"]}}}"#,
            ),
        ],
        prompt: "Run the shell command `env > env.txt`.",
        script: "tool t1 execute Run env; permission t1; say done",
        steps: &[Step::Choose("allow_once")],
        checks: &[
            Check::UserSettings,
            Check::NoFile("hook-ran"),
            Check::NoFile("mcp-ran"),
            Check::NoFile("helper-ran"),
            Check::Lacks("env.txt", "CUPPER_COMPAT_ENV"),
            DONE,
        ],
        ..BASE
    },
    Scenario {
        id: "hooks-reload",
        title: "hooks written in the middle of a turn (Q28)",
        mode: "default",
        real_only: Some("the fake agent reads no `.claude/settings.json`"),
        prompt: "Run the shell command `echo one`, then the shell command `echo two`.",
        steps: &[
            Step::Choose("allow_once"),
            Step::WriteFile(".claude/settings.json", RELOADED),
            Step::Choose("allow_once"),
        ],
        checks: &[Check::Observe("hook-reloaded"), DONE],
        ..BASE
    },
    Scenario {
        id: "config-scope",
        title: "configuration above the project and outside the git root (Q34)",
        mode: "default",
        real_only: Some("the fake agent reads no `.claude/settings.json`"),
        project: Some("sub"),
        files: &[
            ("../.claude/settings.json", OUTSIDE),
            (".claude/settings.json", ROOT),
            ("sub/.keep", ""),
        ],
        prompt: "Reply with the single word: ready",
        checks: &[
            Check::Observe("root-ran"),
            Check::Observe("outside-ran"),
            DONE,
        ],
        ..BASE
    },
];

/// Hooks and settings that leave a marker file when they take effect; the
/// hooks run in the project's directory.
const UNTRUSTED_SETTINGS: &str = r#"{"env": {"CUPPER_COMPAT_ENV": "1"},
    "apiKeyHelper": "touch helper-ran && echo key",
    "hooks": {"UserPromptSubmit": [{"hooks": [{"type": "command", "command": "touch hook-ran"}]}]}}"#;
const RELOADED: &str = r#"{"hooks": {"PreToolUse": [{"matcher": "Bash",
    "hooks": [{"type": "command", "command": "touch hook-reloaded"}]}]}}"#;
const ROOT: &str = r#"{"hooks": {"UserPromptSubmit": [{"hooks": [{"type": "command",
    "command": "touch ../root-ran"}]}]}}"#;
const OUTSIDE: &str = r#"{"hooks": {"UserPromptSubmit": [{"hooks": [{"type": "command",
    "command": "touch ../outside-ran"}]}]}}"#;

const BASE: Scenario = Scenario {
    id: "",
    title: "",
    gold_b: false,
    mode: "",
    loosens: false,
    model: None,
    files: &[],
    skip: None,
    real_only: None,
    project: None,
    untrusted: false,
    prompt: "",
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
            Self::Lacks(path, text) => format!("{path} without {text}"),
            Self::Notice(code) => format!("a {code} notice"),
            Self::NoCrossTalk => "each session holds only its own markers".into(),
            Self::Steady => "no more processes after the resumes".into(),
            Self::Running => "a command ran below the adapter".into(),
            Self::UserSettings => "the session takes only the user's settings".into(),
            Self::ImageSent => "the image went to the agent".into(),
            Self::Sent(method) => format!("the host sent {method}"),
            Self::Observe(path) => format!("{path} exists"),
        }
    }
}
