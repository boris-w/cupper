//! `Coalescer` (design 5.2.6, 5.2.16, 5.3.5): writes the agent's updates
//! into the running turn as item events. Migrated from SP-4a's normaliser.
//!
//! - Text is coalesced at write time (ADR-06): an item's first fragment goes
//!   out at once; later ones wait up to 40 ms, and go out sooner when any
//!   other event is about to be written or a delta is full. Only fragments of
//!   the same item and field merge, so the log reads as if every fragment had
//!   been written alone and merged by `item.delta`'s `Mergeable` rule.
//! - Items carry their activity group; `turn.completed` carries the final
//!   answer ([`TurnItems`]).
//! - Updates that arrive while no turn is open (none yet, or the running
//!   turn's end is decided) wait for the next turn, which the autonomous
//!   judge opens.
//! - When a turn's end is decided, its open items end: text and the plan as
//!   `completed`, tool calls as `abandoned`, with `may_still_run` for the
//!   commands of an interrupted turn when interrupting does not stop them
//!   (Codex). A later update of an abandoned tool call is late output (K3): a
//!   `late_tool_output` item in the turn running then, one per tool call and
//!   turn, referring to the original, whose events stay as they were.
//! - A tool call's `extraction` is `partial` while a field its kind requires
//!   is missing; when it ends, the fields it got by then decide (T017).
//! - A tool's output over the inline limit turns `truncated`: the buffered
//!   deltas go out first, then no more; its tail and size are written at
//!   most once a tick. At its end the actor stores it as a blob
//!   ([`Action::Spill`]), and `item.completed` refers to it; a turn's end
//!   leaves it `abandoned` with its tail (design 5.3.5).

use std::collections::HashMap;
use std::mem;
use std::sync::Arc;
use std::time::Duration;

use cupper_protocol::annotations::{DurationMs, TimestampMs, WirePath};
use cupper_protocol::domain::{BlobContent, BlobRef, Content, TextContent};
use cupper_protocol::event::EventBody;
use cupper_protocol::ids::{GroupId, ItemId, TurnId};
use cupper_protocol::item::{
    DeltaField, Extraction, Item, ItemCompleted, ItemDelta, ItemPatch, ItemStarted, ItemStatus,
    ItemUpdated, LateToolOutput, Notice, NoticeCode, OutputState, Plan, ToolCall, ToolKind,
    ToolRef,
};
use cupper_protocol::limits::V1;
use cupper_protocol::session::TurnCompleted;
use cupper_protocol::text::{
    MAX_DELTA_ESCAPED, MAX_LABEL_CHARS, escaped_len, split_escaped, truncate_chars,
};

use super::grouping::{Role, TurnItems};
use super::turn::{TurnEnd, TurnSignal};
use crate::clock::{IdGen, MonoTime, Now};
use crate::driver::api::{CompactionPhase, NormUpdate, ToolOutput, ToolStatus};
use crate::durations::{Durations, ITEM_COALESCE, OUTPUT_TAIL};
use crate::pure::{Core, TimerRequest};

#[derive(Debug)]
pub struct Coalescer {
    ids: Arc<IdGen>,
    window: Duration,
    stops_commands: bool,
    turn: Option<OpenTurn>,
    held: Vec<NormUpdate>,
    /// By ACP `toolCallId`, for the whole session.
    tools: HashMap<String, Tool>,
    /// Text waiting for the window to end.
    pending: Option<(ItemId, DeltaField, String)>,
    /// Each tool item's output so far, and its escaped length, while inline.
    outputs: HashMap<ItemId, (String, usize)>,
    /// Outputs over the inline limit.
    long: HashMap<ItemId, Long>,
    tick: Duration,
    /// The step being taken: its time and its actions so far.
    now: Now,
    out: Vec<Action>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    Turn(TurnSignal),
    Update(NormUpdate),
    /// Another event goes into the turn next: an interaction request.
    Interject,
    Expired(FlushTimer),
    /// The actor stored a [`Spill`]'s output, or could not.
    Spilled(Box<Spill>, Option<BlobRef>),
}

#[derive(Debug, Clone, PartialEq)]
#[expect(
    clippy::large_enum_variant,
    reason = "a step's few actions live only until the actor carries them out"
)]
pub enum Action {
    /// Append to the session stream.
    Event(EventBody),
    Timer(TimerRequest<FlushTimer>),
    Spill(Box<Spill>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FlushTimer {
    /// The text waiting for the window.
    Text,
    /// The tail of an output over the inline limit.
    Tail(ItemId),
}

/// A tool's output over the inline limit as the tool ends, for the actor to
/// store as a blob; [`Input::Spilled`] brings it back.
#[derive(Debug, Clone, PartialEq)]
pub struct Spill {
    /// The whole output, up to `max_blob_bytes`.
    pub text: String,
    target: Target,
    status: ItemStatus,
    patch: ItemPatch,
}

/// An output over the inline limit.
#[derive(Debug, Default)]
struct Long {
    text: String,
    /// What came, past `max_blob_bytes` too.
    bytes: u64,
    /// A tick is set; the tail changed since it was written.
    ticking: bool,
    due: bool,
}

impl Long {
    fn add(&mut self, text: &str, whole: bool) {
        if whole {
            (self.text, self.bytes) = (String::new(), 0);
        }
        self.bytes += text.len() as u64;
        let mut end = (V1.max_blob_bytes as usize).saturating_sub(self.text.len());
        end = end.min(text.len());
        // Up to the blob's limit, on a character's boundary.
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        self.text.push_str(&text[..end]);
        self.due = true;
    }

    /// Its last 4,000 characters, and its size.
    fn shown(&self, patch: ItemPatch) -> ItemPatch {
        let tail = self.text.char_indices().rev().nth(3_999);
        let tail = &self.text[tail.map_or(0, |(at, _)| at)..];
        ItemPatch {
            output_tail: Some(tail.to_owned()),
            output_bytes: Some(self.bytes),
            ..patch
        }
    }
}

#[derive(Debug)]
struct OpenTurn {
    id: TurnId,
    items: TurnItems,
    /// The message or thought being written, with its ACP message ID.
    text: Option<(Target, Role, Option<String>)>,
    plan: Option<Target>,
    /// The end is decided: nothing more goes in.
    ending: bool,
}

/// An item that can still change.
#[derive(Debug, Clone, PartialEq)]
struct Target {
    id: ItemId,
    started: MonoTime,
    /// Its output streams as deltas.
    streaming: bool,
}

#[derive(Debug)]
struct Tool {
    /// The original item, as late output refers to it.
    of: ToolRef,
    item: Target,
    /// The call's fields so far.
    call: ToolCall,
    phase: ToolPhase,
}

#[derive(Debug)]
enum ToolPhase {
    Running,
    Abandoned,
    /// Its late output item, in that turn.
    Late(Target, TurnId),
    Done,
}

fn label(text: &str) -> String {
    truncate_chars(text, MAX_LABEL_CHARS)
}

impl Coalescer {
    /// `stops_commands`: the agent's `interrupt_stops_commands`.
    pub fn new(ids: Arc<IdGen>, durations: &Durations, stops_commands: bool) -> Self {
        Self {
            ids,
            window: durations.get(ITEM_COALESCE),
            stops_commands,
            turn: None,
            held: Vec::new(),
            tools: HashMap::new(),
            pending: None,
            outputs: HashMap::new(),
            long: HashMap::new(),
            tick: durations.get(OUTPUT_TAIL),
            now: Now {
                mono: MonoTime::ZERO,
                wall_ms: 0,
            },
            out: Vec::new(),
        }
    }

    /// The agent's `interrupt_stops_commands`, once an attach tells it.
    pub fn set_stops_commands(&mut self, stops_commands: bool) {
        self.stops_commands = stops_commands;
    }

    fn flush(&mut self) {
        let Some((item_id, field, text)) = self.pending.take() else {
            return;
        };
        for text in split_escaped(&text, MAX_DELTA_ESCAPED)
            .into_iter()
            .map(str::to_owned)
        {
            let (item_id, field) = (item_id.clone(), field.clone());
            let delta = EventBody::ItemDelta(ItemDelta {
                item_id,
                field,
                text,
            });
            self.out.push(Action::Event(delta));
        }
    }

    /// Writes an event other than a delta: the buffered text goes first.
    fn emit(&mut self, body: EventBody) {
        self.flush();
        self.out.push(Action::Event(body));
    }

    /// Appends `text` to `field` of `item`; a field's `first` text goes out
    /// at once.
    fn append(&mut self, item: &Target, field: DeltaField, text: String, first: bool) {
        if text.is_empty() {
            return;
        }
        match &mut self.pending {
            Some((id, f, buffered)) if *id == item.id && *f == field => buffered.push_str(&text),
            _ => {
                self.flush();
                self.pending = Some((item.id.clone(), field, text));
                let at = self.now.mono + self.window;
                let set = TimerRequest::Set {
                    key: FlushTimer::Text,
                    at,
                };
                self.out.extend((!first).then_some(Action::Timer(set)));
            }
        }
        let full = |(_, _, text): &(_, _, String)| escaped_len(text) >= MAX_DELTA_ESCAPED;
        if first || self.pending.as_ref().is_some_and(full) {
            self.flush();
        }
    }

    fn end(&mut self, item: &Target, status: ItemStatus, may_still_run: bool, patch: ItemPatch) {
        let elapsed = self.now.mono.saturating_duration_since(item.started);
        let completed = ItemCompleted {
            item_id: item.id.clone(),
            status,
            ended_at: TimestampMs(self.now.wall_ms),
            duration_ms: DurationMs(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)),
            may_still_run,
            patch,
        };
        self.emit(EventBody::ItemCompleted(completed));
    }

    fn patch(&mut self, item: &Target, patch: ItemPatch) {
        let item_id = item.id.clone();
        self.emit(EventBody::ItemUpdated(ItemUpdated { item_id, patch }));
    }

    fn open_turn(&mut self) -> &mut OpenTurn {
        self.turn.as_mut().expect("items go into an open turn")
    }

    /// Starts an item in the open turn, ending its message or thought.
    fn start(&mut self, item: Item, role: Role) -> Target {
        if let Some((text, ..)) = self.open_turn().text.take() {
            self.end(&text, ItemStatus::Completed, false, ItemPatch::default());
        }
        let (ids, now) = (Arc::clone(&self.ids), self.now);
        let item_id = ItemId::from_uuid(ids.new_id_at(now.wall_ms).as_u128());
        let mint = || GroupId::from_uuid(ids.new_id_at(now.wall_ms).as_u128());
        let turn = self.open_turn();
        let started = ItemStarted {
            item_id: item_id.clone(),
            turn_id: Some(turn.id.clone()),
            started_at: TimestampMs(now.wall_ms),
            group_id: turn.items.start(&item_id, role, mint),
            item,
        };
        self.emit(EventBody::ItemStarted(started));
        let started = now.mono;
        Target {
            id: item_id,
            started,
            streaming: false,
        }
    }

    fn update(&mut self, update: NormUpdate) {
        if self.turn.as_ref().is_none_or(|turn| turn.ending) {
            return self.held.push(update);
        }
        match update {
            NormUpdate::AgentText { message_id, text } => {
                self.text(Role::Message, message_id, text)
            }
            NormUpdate::Thought { text } => self.text(Role::Thought, None, text),
            NormUpdate::ToolStart {
                tool_call_id,
                tool_kind,
                title,
                paths,
                command,
                cwd,
            } => {
                let tool = tool_call(tool_kind, label(&title), paths, command, cwd);
                self.tool(tool_call_id, Some(tool), None, None, None);
            }
            NormUpdate::ToolUpdate {
                tool_call_id,
                title,
                command,
                paths,
                status,
                output,
                exit_code,
                diffs,
            } => {
                let mut patch = ItemPatch::default();
                (patch.title, patch.exit_code, patch.diffs) = (title, exit_code, diffs);
                (patch.command, patch.paths) = (command, paths);
                self.tool(tool_call_id, None, Some(patch), output, status);
            }
            NormUpdate::Plan(plan) => self.plan(plan),
            NormUpdate::Compaction {
                phase: CompactionPhase::Started,
                ..
            } => {
                let notice = Notice {
                    code: NoticeCode::Compaction,
                    params: Default::default(),
                };
                let item = self.start(Item::Notice(notice), Role::Other);
                self.end(&item, ItemStatus::Completed, false, ItemPatch::default());
            }
            NormUpdate::Compaction { .. } => {}
        }
    }

    fn text(&mut self, role: Role, message_id: Option<String>, text: String) {
        if let Some((open, r, m)) = &self.open_turn().text
            && (*r, m) == (role, &message_id)
        {
            let open = open.clone();
            return self.append(&open, DeltaField::Text, text, false);
        }
        let item = if role == Role::Message {
            Item::AgentMessage
        } else {
            Item::Thought
        };
        let open = self.start(item, role);
        self.append(&open, DeltaField::Text, text, true);
        self.open_turn().text = Some((open, role, message_id));
    }

    /// The item of the tool call the agent calls `tool_call_id`.
    pub fn item_of(&self, tool_call_id: &str) -> Option<ItemId> {
        let tool = self.tools.get(tool_call_id)?;
        Some(tool.of.item_id.clone())
    }

    /// A tool call starts, or changes by `patch`, `output` and `status`. A
    /// start of a known call only renames it; an update of an unknown one
    /// starts it.
    fn tool(
        &mut self,
        call: String,
        start: Option<ToolCall>,
        mut patch: Option<ItemPatch>,
        output: Option<ToolOutput>,
        status: Option<ToolStatus>,
    ) {
        if !self.tools.contains_key(&call) {
            let title = patch
                .as_ref()
                .and_then(|p| p.title.clone())
                .unwrap_or_else(|| call.clone());
            let tool = start.unwrap_or_else(|| {
                tool_call(ToolKind::Other, label(&title), Vec::new(), None, None)
            });
            let (call_fields, title, command) =
                (tool.clone(), tool.title.clone(), tool.command.clone());
            let item = self.start(Item::ToolCall(tool), Role::ToolCall);
            let turn_id = self.open_turn().id.clone();
            let of = ToolRef {
                item_id: item.id.clone(),
                turn_id,
                title,
                command,
            };
            let phase = ToolPhase::Running;
            self.tools.insert(
                call.clone(),
                Tool {
                    of,
                    item,
                    call: call_fields,
                    phase,
                },
            );
        } else if let Some(start) = start {
            patch = Some(ItemPatch {
                title: Some(start.title),
                ..ItemPatch::default()
            });
        }
        let mut tool = self.tools.remove(&call).expect("a known tool call");
        let turn_id = self.open_turn().id.clone();
        match &tool.phase {
            ToolPhase::Done => {}
            ToolPhase::Running => self.change(&mut tool, patch.unwrap_or_default(), output, status),
            ToolPhase::Late(_, turn) if *turn == turn_id => {
                self.change(&mut tool, late_patch(patch), output, status)
            }
            ToolPhase::Abandoned | ToolPhase::Late(..) => {
                let late = LateToolOutput {
                    of: tool.of.clone(),
                    output: None,
                    output_state: OutputState::None,
                    output_tail: None,
                    output_bytes: None,
                    exit_code: None,
                    signal: None,
                };
                let item = self.start(Item::LateToolOutput(late), Role::Other);
                tool.phase = ToolPhase::Late(item, turn_id);
                self.change(&mut tool, late_patch(patch), output, status);
            }
        }
        self.tools.insert(call, tool);
    }

    /// Changes the tool call's item, or its late output item.
    fn change(
        &mut self,
        tool: &mut Tool,
        mut patch: ItemPatch,
        output: Option<ToolOutput>,
        status: Option<ToolStatus>,
    ) {
        let running = matches!(tool.phase, ToolPhase::Running);
        if running {
            merge(&mut tool.call, &patch);
        }
        let target = match &mut tool.phase {
            ToolPhase::Late(item, _) => item,
            _ => &mut tool.item,
        };
        match output {
            Some(ToolOutput::Append(text)) => self.output(target, text, false, &mut patch),
            Some(ToolOutput::Replace(text)) => self.output(target, text, true, &mut patch),
            None => {}
        }
        let status = match status {
            Some(ToolStatus::Completed) => ItemStatus::Completed,
            Some(ToolStatus::Failed) => ItemStatus::Failed,
            _ if patch == ItemPatch::default() => return,
            _ => return self.patch(target, patch),
        };
        if running {
            let extraction = match tool.call.missing_required(true).is_empty() {
                true => Extraction::Full,
                false => Extraction::Partial,
            };
            patch.extraction = (extraction != tool.call.extraction).then_some(extraction);
        }
        let target = target.clone();
        match self.output_ended(&target.id) {
            Some(long) => {
                let patch = long.shown(patch);
                let (text, status) = (long.text, status);
                let spill = Box::new(Spill {
                    text,
                    target,
                    status,
                    patch,
                });
                self.out.push(Action::Spill(spill));
            }
            None => self.end(&target, status, false, patch),
        }
        tool.phase = ToolPhase::Done;
    }

    /// A tool's output: deltas while it stays within the inline limit, then
    /// `truncated`. A whole output that fits goes inline at once.
    fn output(&mut self, target: &mut Target, text: String, whole: bool, patch: &mut ItemPatch) {
        let id = target.id.clone();
        if let Some(long) = self.long.get_mut(&id) {
            long.add(&text, whole);
            return self.tail(&id, false);
        }
        let (so_far, escaped) = self.outputs.entry(id.clone()).or_default();
        if whole {
            so_far.clear();
            *escaped = 0;
        }
        let fits = *escaped + escaped_len(&text) <= V1.max_inline_bytes as usize;
        if fits && whole {
            patch.output = Some(Content::Text(TextContent { text }));
            patch.output_state = Some(OutputState::Complete);
        } else if fits {
            so_far.push_str(&text);
            *escaped += escaped_len(&text);
            let first = !mem::replace(&mut target.streaming, true);
            if first {
                let state = Some(OutputState::Streaming);
                let patch = ItemPatch {
                    output_state: state,
                    ..ItemPatch::default()
                };
                self.patch(target, patch);
            }
            self.append(target, DeltaField::Output, text, first);
        } else {
            let mut long = Long::default();
            long.add(&self.outputs.remove(&id).unwrap_or_default().0, true);
            long.add(&text, false);
            self.long.insert(id.clone(), long);
            self.tail(&id, false);
        }
    }

    /// Writes an output's tail and size, at most once a tick: a change
    /// within a tick waits for its end.
    fn tail(&mut self, id: &ItemId, tick: bool) {
        let Some(long) = self.long.get_mut(id) else {
            return;
        };
        long.ticking &= !tick;
        if long.ticking || !long.due {
            return;
        }
        (long.ticking, long.due) = (true, false);
        let state = Some(OutputState::Truncated);
        let patch = long.shown(ItemPatch {
            output_state: state,
            ..ItemPatch::default()
        });
        let item_id = id.clone();
        self.emit(EventBody::ItemUpdated(ItemUpdated { item_id, patch }));
        let (key, at) = (FlushTimer::Tail(id.clone()), self.now.mono + self.tick);
        self.out.push(Action::Timer(TimerRequest::Set { key, at }));
    }

    /// The tool item ends: its output over the inline limit, if it was.
    fn output_ended(&mut self, id: &ItemId) -> Option<Long> {
        self.outputs.remove(id);
        let long = self.long.remove(id)?;
        let cancel = TimerRequest::Cancel(FlushTimer::Tail(id.clone()));
        self.out.push(Action::Timer(cancel));
        Some(long)
    }

    /// The actor stored the output, or could not: then it stays truncated.
    fn spilled(&mut self, spill: Spill, blob: Option<BlobRef>) {
        let (preview, mut patch) = (spill.patch.output_tail.clone(), spill.patch);
        let blob = blob.map(|blob| BlobContent {
            blob,
            preview: preview.unwrap_or_default(),
        });
        patch.output_state = Some(match blob {
            Some(_) => OutputState::Complete,
            None => OutputState::Truncated,
        });
        patch.output = blob.map(Content::Blob);
        self.end(&spill.target, spill.status, false, patch);
    }

    fn plan(&mut self, plan: Plan) {
        if let Some(item) = self.open_turn().plan.clone() {
            let mut patch = ItemPatch::default();
            (patch.source, patch.entries, patch.markdown) =
                (Some(plan.source), plan.entries, plan.markdown);
            return self.patch(&item, patch);
        }
        let item = self.start(Item::Plan(plan), Role::Other);
        self.open_turn().plan = Some(item);
    }

    /// The turn's end is decided: its open items end.
    fn settle(&mut self, interrupted: bool) {
        let Some(turn) = &mut self.turn else {
            return;
        };
        turn.ending = true;
        let (turn_id, text, plan) = (turn.id.clone(), turn.text.take(), turn.plan.take());
        for item in text.map(|(item, ..)| item).into_iter().chain(plan) {
            self.end(&item, ItemStatus::Completed, false, ItemPatch::default());
        }
        let mut open: Vec<(Target, String)> = (self.tools.iter())
            .filter_map(|(call, tool)| match &tool.phase {
                ToolPhase::Running if tool.of.turn_id == turn_id => Some((tool.item.clone(), call)),
                ToolPhase::Late(item, turn) if *turn == turn_id => Some((item.clone(), call)),
                _ => None,
            })
            .map(|(item, call)| (item, call.clone()))
            .collect();
        open.sort_by(|(a, _), (b, _)| a.id.cmp(&b.id));
        for (item, call) in open {
            let tool = self.tools.get_mut(&call).expect("an open tool call");
            tool.phase = ToolPhase::Abandoned;
            let execute = tool.call.tool_kind == ToolKind::Execute;
            let may_still_run = interrupted && !self.stops_commands && execute;
            let output = self.output_ended(&item.id);
            let patch = output.map_or_else(ItemPatch::default, |l| l.shown(ItemPatch::default()));
            self.end(&item, ItemStatus::Abandoned, may_still_run, patch);
        }
    }

    fn completed(&mut self, end: TurnEnd) {
        let turn = self.turn.take().filter(|turn| turn.id == end.turn_id);
        let completed = TurnCompleted {
            turn_id: end.turn_id,
            status: end.status,
            stop_reason: end.stop_reason,
            ended_at: end.ended_at,
            duration_ms: end.duration_ms,
            usage: end.usage,
            error_code: end.error_code,
            final_item_ids: turn
                .map(|turn| turn.items.final_items())
                .unwrap_or_default(),
        };
        self.emit(EventBody::TurnCompleted(completed));
    }
}

/// Takes the fields of `patch` that decide a call's `extraction`.
fn merge(call: &mut ToolCall, patch: &ItemPatch) {
    if let Some(title) = &patch.title {
        call.title.clone_from(title);
    }
    if let Some(paths) = &patch.paths {
        call.paths.clone_from(paths);
    }
    call.command = patch.command.clone().or(call.command.take());
    call.exit_code = patch.exit_code.or(call.exit_code);
}

/// A late output item keeps its reference: only the exit code changes.
fn late_patch(patch: Option<ItemPatch>) -> ItemPatch {
    let exit_code = patch.and_then(|patch| patch.exit_code);
    ItemPatch {
        exit_code,
        ..ItemPatch::default()
    }
}

/// A tool call as it starts; `extraction` is `partial` while a field its
/// kind requires is missing.
fn tool_call(
    kind: ToolKind,
    title: String,
    paths: Vec<WirePath>,
    command: Option<String>,
    cwd: Option<WirePath>,
) -> ToolCall {
    let mut tool = ToolCall {
        tool_kind: kind,
        title,
        paths,
        query: None,
        url: None,
        command,
        cwd,
        exit_code: None,
        signal: None,
        output: None,
        output_state: OutputState::None,
        output_tail: None,
        output_bytes: None,
        diffs: None,
        parent_item_id: None,
        extraction: Extraction::Full,
    };
    if !tool.missing_required(false).is_empty() {
        tool.extraction = Extraction::Partial;
    }
    tool
}

impl Core for Coalescer {
    type Input = Input;
    type Action = Action;
    type TimerKey = FlushTimer;

    fn step(&mut self, input: Input, now: Now) -> Vec<Action> {
        self.now = now;
        match input {
            Input::Turn(TurnSignal::Started { turn_id: id, .. }) => {
                let (items, text, plan) = (TurnItems::default(), None, None);
                self.turn = Some(OpenTurn {
                    id,
                    items,
                    text,
                    plan,
                    ending: false,
                });
                for update in mem::take(&mut self.held) {
                    self.update(update);
                }
            }
            Input::Turn(TurnSignal::Settled { interrupted }) => self.settle(interrupted),
            Input::Turn(TurnSignal::Completed(end)) => self.completed(end),
            Input::Update(update) => self.update(update),
            Input::Interject => {
                self.flush();
                self.turn.iter_mut().for_each(|turn| turn.items.end_group());
            }
            Input::Expired(FlushTimer::Text) => self.flush(),
            Input::Expired(FlushTimer::Tail(id)) => self.tail(&id, true),
            Input::Spilled(spill, blob) => self.spilled(*spill, blob),
        }
        mem::take(&mut self.out)
    }

    fn timer_request(action: &Action) -> Option<TimerRequest<FlushTimer>> {
        match action {
            Action::Timer(request) => Some(request.clone()),
            Action::Event(_) | Action::Spill(_) => None,
        }
    }

    fn timer_expired(key: FlushTimer) -> Input {
        Input::Expired(key)
    }
}

#[cfg(test)]
mod tests {
    use cupper_protocol::coalesce::Mergeable;
    use cupper_protocol::item::PlanSource;
    use cupper_protocol::session::{TurnOrigin, TurnStatus, UsageUpdated};
    use proptest::prelude::*;

    use super::*;
    use crate::clock::ManualClock;
    use crate::pure::{CoreDriver, Step};

    const WINDOW: Duration = Duration::from_millis(40);

    fn coalescer() -> CoreDriver<Coalescer> {
        let clock = ManualClock::new();
        let ids = Arc::new(IdGen::seeded(Arc::new(clock.clone()), 7));
        CoreDriver::with_clock(Coalescer::new(ids, &Durations::default(), false), clock)
    }

    fn started(turn: u128) -> Input {
        let (turn_id, origin) = (TurnId::from_uuid(turn), TurnOrigin::User);
        Input::Turn(TurnSignal::Started { turn_id, origin })
    }

    fn completed(turn: u128) -> Input {
        Input::Turn(TurnSignal::Completed(TurnEnd {
            turn_id: TurnId::from_uuid(turn),
            status: TurnStatus::Completed,
            stop_reason: None,
            error_code: None,
            usage: None,
            ended_at: TimestampMs(0),
            duration_ms: DurationMs(0),
        }))
    }

    fn tool(id: &str, output: Option<ToolOutput>, status: Option<ToolStatus>) -> Input {
        let (tool_call_id, title, diffs) = (id.to_owned(), None, None);
        let exit_code = status.map(|_| 0);
        let update = NormUpdate::ToolUpdate {
            tool_call_id,
            title,
            command: None,
            paths: None,
            status,
            output,
            exit_code,
            diffs,
        };
        Input::Update(update)
    }

    fn tool_start(id: &str) -> Input {
        Input::Update(NormUpdate::ToolStart {
            tool_call_id: id.to_owned(),
            tool_kind: ToolKind::Execute,
            title: "Run tests".to_owned(),
            paths: Vec::new(),
            command: Some("cargo test".to_owned()),
            cwd: Some(WirePath("/work".to_owned())),
        })
    }

    fn message(id: Option<&str>, text: &str) -> Input {
        let (message_id, text) = (id.map(str::to_owned), text.to_owned());
        Input::Update(NormUpdate::AgentText { message_id, text })
    }

    fn events(actions: Vec<Action>) -> Vec<EventBody> {
        let event = |action| match action {
            Action::Event(body) => Some(body),
            Action::Timer(_) | Action::Spill(_) => None,
        };
        actions.into_iter().filter_map(event).collect()
    }

    /// Contiguous deltas of one item and field, merged as `item.delta` merges.
    fn merged(log: &[EventBody]) -> Vec<EventBody> {
        let mut out: Vec<EventBody> = Vec::new();
        for body in log {
            match out.last_mut().and_then(|last| last.merge(body, usize::MAX)) {
                Some(merged) => *out.last_mut().unwrap() = merged,
                None => out.push(body.clone()),
            }
        }
        out
    }

    /// Claude's command arrives in an update after the start (T053); the
    /// fields the call has when it ends decide its `extraction` (T017).
    #[test]
    fn coalescer_tool_extraction_follows_the_fields_a_call_ends_with() {
        let mut c = coalescer();
        c.input(started(1));
        let (tool_kind, paths, cwd) = (ToolKind::Execute, Vec::new(), Some(WirePath("/w".into())));
        for id in ["full", "partial"] {
            let (tool_call_id, title, command) = (id.to_owned(), "Terminal".to_owned(), None);
            let start = NormUpdate::ToolStart {
                tool_call_id,
                tool_kind: tool_kind.clone(),
                title,
                paths: paths.clone(),
                command,
                cwd: cwd.clone(),
            };
            let start = events(c.input(Input::Update(start)));
            let [
                EventBody::ItemStarted(ItemStarted {
                    item: Item::ToolCall(call),
                    ..
                }),
            ] = &start[..]
            else {
                panic!("{start:?}")
            };
            assert_eq!((&call.extraction, &call.cwd), (&Extraction::Partial, &cwd));
        }
        let mut update = |id: &str, command: Option<&str>, exit_code, status| {
            let update = NormUpdate::ToolUpdate {
                tool_call_id: id.to_owned(),
                title: command.map(str::to_owned),
                command: command.map(str::to_owned),
                paths: None,
                status,
                output: None,
                exit_code,
                diffs: None,
            };
            events(c.input(Input::Update(update)))
        };
        let ended = |events: Vec<EventBody>| match &events[..] {
            [EventBody::ItemCompleted(end)] => end.patch.extraction.clone(),
            _ => panic!("{events:?}"),
        };
        update("full", Some("ls"), None, None);
        let done = update("full", None, Some(0), Some(ToolStatus::Completed));
        assert_eq!(ended(done), Some(Extraction::Full));
        // No exit code: still partial, so nothing to change.
        update("partial", Some("ls"), None, None);
        let done = update("partial", None, None, Some(ToolStatus::Failed));
        assert_eq!(ended(done), None);
    }

    #[test]
    fn coalescer_settles_open_items_and_writes_late_output_in_the_next_turn() {
        let mut c = coalescer();
        c.input(started(1));
        c.input(message(Some("m1"), "Running"));
        c.input(tool_start("t1"));
        c.input(tool("t1", Some(ToolOutput::Append("a".into())), None));
        let settle = events(c.input(Input::Turn(TurnSignal::Settled { interrupted: true })));
        let [EventBody::ItemCompleted(abandoned)] = &settle[..] else {
            panic!("{settle:?}")
        };
        assert_eq!(
            (&abandoned.status, abandoned.may_still_run),
            (&ItemStatus::Abandoned, true)
        );
        // Held while the turn ends, written into the next turn.
        assert!(
            c.input(tool("t1", Some(ToolOutput::Append("b".into())), None))
                .is_empty()
        );
        let end = events(c.input(completed(1)));
        let [EventBody::TurnCompleted(end)] = &end[..] else {
            panic!("{end:?}")
        };
        assert_eq!(end.final_item_ids.len(), 0, "the turn ended on a tool call");
        let late = events(c.input(started(2)));
        let kinds: Vec<&str> = late.iter().map(EventBody::tag).collect();
        assert_eq!(kinds, ["item.started", "item.updated", "item.delta"]);
        let EventBody::ItemStarted(ItemStarted {
            item: Item::LateToolOutput(late),
            ..
        }) = &late[0]
        else {
            panic!("{late:?}")
        };
        let original = (
            &late.of.item_id,
            &late.of.turn_id,
            late.of.command.as_deref(),
        );
        assert_eq!(
            original,
            (
                &abandoned.item_id,
                &TurnId::from_uuid(1),
                Some("cargo test")
            )
        );
        let done = events(c.input(tool("t1", None, Some(ToolStatus::Completed))));
        let [EventBody::ItemCompleted(done)] = &done[..] else {
            panic!("{done:?}")
        };
        assert_eq!(
            (&done.status, done.patch.exit_code),
            (&ItemStatus::Completed, Some(0))
        );
        c.input(message(Some("m2"), "Done"));
        c.input(Input::Turn(TurnSignal::Settled { interrupted: false }));
        let end = events(c.input(completed(2)));
        let [EventBody::TurnCompleted(end)] = &end[..] else {
            panic!("{end:?}")
        };
        assert_eq!(end.final_item_ids.len(), 1, "late output takes no part");
    }

    #[derive(Debug, Clone)]
    enum Op {
        /// A message fragment; message ID 3 is none.
        Text(u8, String),
        Thought(String),
        /// A tool call starts; when `n % 4 == 0`, one that started before.
        Start(u8),
        Output(u8, String),
        Replace(u8),
        Done(u8),
        Plan,
        /// An interaction request: the coalescer flushes, a marker follows.
        Interject,
        Settle(bool),
        /// The settled turn completes and the next one starts.
        Next,
        Advance(u64),
    }

    fn ops() -> impl Strategy<Value = Vec<Op>> {
        let text = || {
            prop_oneof![
                8 => "\\PC{0,8}|[\"\\\\\\n\\x01é]{0,6}",
                1 => (0..3_000_usize).prop_map(|n| "\u{1}é".repeat(n)),
            ]
        };
        let op = prop_oneof![
            4 => (0..4_u8, text()).prop_map(|(m, t)| Op::Text(m, t)),
            2 => text().prop_map(Op::Thought),
            2 => any::<u8>().prop_map(Op::Start),
            8 => (any::<u8>(), text()).prop_map(|(n, t)| Op::Output(n, t)),
            1 => any::<u8>().prop_map(Op::Replace),
            2 => any::<u8>().prop_map(Op::Done),
            1 => Just(Op::Plan),
            2 => Just(Op::Interject),
            1 => any::<bool>().prop_map(Op::Settle),
            1 => Just(Op::Next),
            4 => prop_oneof![0..40_u64, 40..100_u64].prop_map(Op::Advance),
        ];
        prop::collection::vec(op, 0..80)
    }

    /// The same inputs go to `a`, which coalesces, and to `b`, which is made
    /// to write every fragment at once by a flush after each input.
    struct World {
        a: CoreDriver<Coalescer>,
        b: CoreDriver<Coalescer>,
        logs: [Vec<EventBody>; 2],
        tools: Vec<String>,
        turn: u128,
        ending: bool,
        /// The flush timer's deadline while text waits.
        deadline: Option<MonoTime>,
    }

    impl World {
        fn new() -> Self {
            let (mut a, mut b) = (coalescer(), coalescer());
            a.input(started(1));
            b.input(started(1));
            let (logs, tools) = (Default::default(), Vec::new());
            Self {
                a,
                b,
                logs,
                tools,
                turn: 1,
                ending: false,
                deadline: None,
            }
        }

        /// One of the last three tool calls, so that their outputs interleave.
        fn pick(&self, n: u8) -> Option<String> {
            let back = usize::from(n) % self.tools.len().clamp(1, 3);
            self.tools.iter().rev().nth(back).cloned()
        }

        fn inputs(&mut self, op: Op) -> Vec<Input> {
            let update =
                |id: Option<String>, output, status| id.map(|id| tool(&id, output, status));
            let input = match op {
                Op::Text(m, text) => {
                    message(["m0", "m1", "m2"].get(usize::from(m)).copied(), &text)
                }
                Op::Thought(text) => Input::Update(NormUpdate::Thought { text }),
                Op::Start(n) => match self.pick(n).filter(|_| n % 4 == 0) {
                    Some(id) => tool_start(&id),
                    None => {
                        self.tools.push(format!("t{}", self.tools.len()));
                        tool_start(&self.tools[self.tools.len() - 1])
                    }
                },
                Op::Output(n, text) => {
                    return update(self.pick(n), Some(ToolOutput::Append(text)), None)
                        .into_iter()
                        .collect();
                }
                Op::Replace(n) => {
                    let output = Some(ToolOutput::Replace("all".into()));
                    return update(self.pick(n), output, None).into_iter().collect();
                }
                Op::Done(n) => {
                    let status = Some(ToolStatus::Completed);
                    return update(self.pick(n), None, status).into_iter().collect();
                }
                Op::Plan => {
                    let markdown = Some(Content::Text(TextContent {
                        text: "1. Test".into(),
                    }));
                    let source = PlanSource::Markdown;
                    Input::Update(NormUpdate::Plan(Plan {
                        source,
                        entries: None,
                        markdown,
                    }))
                }
                Op::Interject => Input::Interject,
                Op::Settle(interrupted) if !self.ending => {
                    self.ending = true;
                    Input::Turn(TurnSignal::Settled { interrupted })
                }
                Op::Next if self.ending => {
                    (self.ending, self.turn) = (false, self.turn + 1);
                    return vec![completed(self.turn - 1), started(self.turn)];
                }
                Op::Settle(_) | Op::Next | Op::Advance(_) => return Vec::new(),
            };
            vec![input]
        }

        fn apply(&mut self, op: Op) {
            let mut wrote = false;
            if let Op::Advance(ms) = op {
                let by = Duration::from_millis(ms);
                for (at, action) in self.a.run([Step::Advance(by)]) {
                    assert_eq!(
                        Some(at),
                        self.deadline,
                        "text goes out when its window ends"
                    );
                    self.logs[0].extend(events(vec![action]));
                    wrote = true;
                }
                self.logs[1].extend(events(self.b.advance(by)));
            }
            let interject = matches!(op, Op::Interject);
            for input in self.inputs(op) {
                let written = events(self.a.input(input.clone()));
                wrote |= written.iter().any(|e| matches!(e, EventBody::ItemDelta(_)));
                self.logs[0].extend(written);
                self.logs[1].extend(events(self.b.input(input)));
                self.logs[1].extend(events(self.b.input(Input::Expired(FlushTimer::Text))));
            }
            if interject {
                assert!(
                    self.a.core().pending.is_none(),
                    "flushed before another event"
                );
                let marker = UsageUpdated {
                    context_used: 0,
                    context_size: 0,
                    cost: None,
                };
                let marker = EventBody::UsageUpdated(marker);
                self.logs
                    .iter_mut()
                    .for_each(|log| log.push(marker.clone()));
            }
            self.check(wrote);
        }

        /// Text waits from its first buffered fragment until the window ends,
        /// and the log, with the waiting text, merges to `b`'s.
        fn check(&mut self, wrote: bool) {
            let (now, pending) = (self.a.now().mono, self.a.core().pending.clone());
            self.deadline = pending.as_ref().map(|_| {
                let at = self
                    .a
                    .timers()
                    .iter()
                    .next()
                    .expect("waiting text has a timer")
                    .0;
                let window_end = match self.deadline {
                    Some(before) if !wrote => before,
                    _ => now + WINDOW,
                };
                assert_eq!(at, window_end, "text waits 40 ms from its first fragment");
                at
            });
            let mut log = self.logs[0].clone();
            let delta = |(item_id, field, text)| {
                EventBody::ItemDelta(ItemDelta {
                    item_id,
                    field,
                    text,
                })
            };
            log.extend(pending.map(delta));
            assert_eq!(
                merged(&log),
                merged(&self.logs[1]),
                "the log reads as written one by one"
            );
        }
    }

    proptest! {
        #[test]
        fn prop_coalescer_log_reads_as_written_fragment_by_fragment(ops in ops()) {
            let mut world = World::new();
            for op in ops {
                world.apply(op);
            }
            world.apply(Op::Advance(40));
            prop_assert!(world.a.core().pending.is_none());
            for body in &world.logs[0] {
                if let EventBody::ItemDelta(delta) = body {
                    prop_assert!(escaped_len(&delta.text) <= MAX_DELTA_ESCAPED);
                }
            }
        }
    }
}
