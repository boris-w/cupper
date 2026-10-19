//! UNIT-policy-matrix: the permission matrix of design 5.8.3, cell by cell,
//! as the method level and the resource level decide it together.
//!
//! [`MATRIX`] follows `MATRIX` in the tests of `cupper-protocol`'s
//! `methods/catalog.rs` row by row; that one checks the scopes alone, here a
//! cell is the policy's verdict on a typical request of the row. [`FORMS`]
//! adds the rows 5.8.3 has for one form of a method.

use std::collections::BTreeSet;

use cupper_protocol::annotations::{X_BUILDS, X_SCOPE};
use cupper_protocol::domain::Tier;
use cupper_protocol::interaction::{InteractionOutcome, Selected};
use cupper_protocol::methods::admin::PolicyDescribeParams;
use cupper_protocol::methods::catalog::catalog;
use cupper_protocol::methods::handshake::Principal;
use cupper_protocol::rpc::Scope;
use cupper_protocol::schema::generator;
use serde_json::{Value, json};

use super::testkit::{COLUMNS, column, option, permission, respond_params};
use super::*;
use crate::clock::MonoTime;

/// The methods of a row of 5.8.3, space-separated, and one letter per column:
/// - `Y` ✓; `N` ✗ for lack of a scope;
/// - `C` "原生确认": allowed to the control connection, which holds the
///   scope; the command is one `policy/describe` describes;
/// - `M` "仅由主进程代发": allowed;
/// - `O` ✗ but for `cli`; `S` ✗ by SEC-9;
/// - `I` "真实输入": allowed with the window's input evidence, not without;
/// - `B` "仅 … 构建": allowed in the builds that register the method.
const MATRIX: &[(&str, &str)] = &[
    ("initialize", "YMYYYYY"),
    ("ping client/presence host/status", "YYYYYYY"),
    (
        "stream/subscribe stream/unsubscribe stream/ack history/page blob/get",
        "YYYYYYY",
    ),
    ("blob/put", "YYNYNYY"),
    ("host/prepare_upgrade host/exit", "CNNNNYN"),
    (
        "settings/get git/status git/diff git/file_diff git/branches worktree/check files/search",
        "YYYYYYY",
    ),
    (
        "agents/capabilities project/trust_info session/get session/capabilities session/list_archived interaction/get",
        "YYYYYYY",
    ),
    ("git/stage git/unstage git/commit git/push", "YYNYNYS"),
    ("git/revert", "YINYNYS"),
    ("settings/update", "YYNYNYS"),
    ("workspace/open diag/export", "YYNNNYN"),
    ("auth/elevate", "OOYYYOO"),
    ("auth/elevation_get policy/describe", "YNNNNYN"),
    ("auth/elevation_respond", "CNNNNYN"),
    ("diag/recording_export", "BBNNNYN"),
    (
        "test/clock_advance test/run_maintenance test/seed",
        "NNNNNBN",
    ),
    ("project/add project/remove", "YYNYNYS"),
    ("project/trust", "CNNNNYN"),
    (
        "session/set_config session/rename session/archive session/restore agents/refresh agents/restart",
        "YYNYNYS",
    ),
    ("session/mark_read", "YYYYYYY"),
    (
        "session/create session/send session/interrupt session/queue_resume session/queue_edit",
        "YYNYNYY",
    ),
    ("session/confirm_config", "CNNNNYN"),
    ("interaction/respond", "YINNYYY"),
];

/// What the verdict sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Form {
    /// The row's typical request: an ordinary setting; an option that sets
    /// neither the tier nor the model; an `approve` option; a new session
    /// at `normal`.
    Typical,
    /// `settings/update` of a security-related key.
    SecurityKey,
    /// `interaction/respond` choosing a `requires: privileged` option.
    PrivilegedOption,
    /// `session/set_config` of the model.
    Model,
}

/// The rows of 5.8.3 for one form of a method.
const FORMS: &[(&str, Form, &str)] = &[
    ("settings/update", Form::SecurityKey, "CNNNNYS"),
    ("interaction/respond", Form::PrivilegedOption, "CNNNNYN"),
    // "remote_device 仅 set_config 的模型和推理强度" (MB-9).
    ("session/set_config", Form::Model, "YYNYNYY"),
];

/// The policy's verdict on `principal` sending `method`, whose scope is
/// `scope`, in `form`, with the window's input evidence or without.
fn verdict(
    principal: &Principal,
    method: &str,
    scope: Option<&Scope>,
    form: Form,
    input: bool,
) -> Result<(), Denial> {
    super::method(principal, method, scope)?;
    let evidence = input.then(|| json!({"kind": "click"}));
    match method {
        "settings/update" => {
            let patch = match form {
                Form::SecurityKey => json!({"diagnostics.verbose": true}),
                _ => json!({"notifications.error": true}),
            };
            settings_update(
                principal,
                &params(json!({"command_id": "c-1", "patch": patch})),
            )
        }
        "session/set_config" => {
            let option = if form == Form::Model {
                "model"
            } else {
                "plan_mode"
            };
            let raw = json!({"command_id": "c-1", "session_id": "s_1", "option": option,
                "value": "x"});
            set_config(principal, &params(raw))
        }
        "git/revert" => {
            let mut raw = json!({"command_id": "c-1", "session_id": "s_1",
                "diff_token": "dt_1", "target": {"paths": ["a.rs"]}});
            if let Some(evidence) = evidence {
                raw["input"] = evidence;
            }
            git_revert(principal, &params(raw))
        }
        "interaction/respond" => {
            let request = permission(vec![
                option("always", "allow_always", "persistent", "privileged"),
                option("once", "allow_once", "once", "approve"),
            ]);
            let pending = Pending {
                request: &request,
                entered: MonoTime::ZERO,
                tier: Some(&Tier::Normal),
            };
            let chosen = if form == Form::PrivilegedOption {
                "always"
            } else {
                "once"
            };
            let outcome = InteractionOutcome::Selected(Selected {
                option_id: chosen.into(),
            });
            let at = MonoTime::ZERO + MIN_ANSWER_DELAY;
            respond(principal, &pending, &respond_params(outcome, evidence), at)
        }
        "session/create" => tier_change(principal, None, &Tier::Normal),
        _ => Ok(()),
    }
}

fn params<T: serde::de::DeserializeOwned>(raw: Value) -> T {
    serde_json::from_value(raw).unwrap()
}

/// Checks the cells of `method` in `form`; `entry` is its catalog entry.
fn check(method: &str, entry: &Value, form: Form, cells: &str) {
    let scope = match entry[X_SCOPE].as_str().unwrap() {
        "none" => None,
        scope => Some(Scope::from(scope)),
    };
    assert_eq!(cells.len(), COLUMNS.len(), "{method}");
    for (name, cell) in COLUMNS.into_iter().zip(cells.chars()) {
        let principal = column(name);
        let with = verdict(&principal, method, scope.as_ref(), form, true);
        let without = verdict(&principal, method, scope.as_ref(), form, false);
        let at = format!("5.8.3 {method} ({form:?}) for {name}");
        match cell {
            'Y' => assert_eq!((with, without), (Ok(()), Ok(())), "{at}"),
            'M' => {
                assert_eq!(method, "initialize", "{at}");
                assert_eq!(with, Ok(()), "{at}");
            }
            'B' => {
                assert!(entry.get(X_BUILDS).is_some(), "{at}");
                assert_eq!(with, Ok(()), "{at}");
            }
            'C' => {
                assert_eq!(name, "UI control", "{at}");
                assert_eq!((with, without), (Ok(()), Ok(())), "{at}");
                let described = Confirmable::parse(&PolicyDescribeParams {
                    method: method.into(),
                    params: json!({}),
                });
                assert!(matches!(described, Err(DescribeError::Params(_))), "{at}");
            }
            'I' => {
                assert_eq!(name, "UI window", "{at}");
                assert_eq!(with, Ok(()), "{at}");
                assert_eq!(without, Err(Denial::UserActivation), "{at}");
            }
            'N' => assert!(matches!(with, Err(Denial::Scope(_))), "{at}: {with:?}"),
            'O' => assert_eq!(with, Err(Denial::CliOnly), "{at}"),
            'S' => assert_eq!(with, Err(Denial::Sec9), "{at}"),
            _ => panic!("{at}: no cell {cell}"),
        }
    }
}

#[test]
fn unit_policy_matrix_every_cell() {
    let catalog = catalog(&mut generator());
    let entries: Vec<(&&str, &Value)> = catalog
        .requests
        .iter()
        .chain(&catalog.client_notifications)
        .collect();
    let mut covered = BTreeSet::new();
    for (methods, cells) in MATRIX {
        for method in methods.split(' ') {
            assert!(covered.insert(method), "{method} in two rows");
            let (_, entry) = entries.iter().find(|(name, _)| **name == method).unwrap();
            check(method, entry, Form::Typical, cells);
        }
    }
    let all: BTreeSet<&str> = entries.iter().map(|(name, _)| **name).collect();
    assert_eq!(covered, all, "every method sits in one row");
}

#[test]
fn unit_policy_matrix_forms_of_a_method() {
    let catalog = catalog(&mut generator());
    for (method, form, cells) in FORMS {
        check(method, &catalog.requests[method], *form, cells);
    }
}
