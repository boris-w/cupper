//! IT-runtime-install, the parts without a download (SP-2's failures and a
//! bundle replaced during the copy), on a bundle made here: `Helpers/node` is
//! a copy of a signed system binary, esbuild-wasm a few small files. The
//! statuses each run publishes show the core's transitions.

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::sync::atomic::{AtomicUsize, Ordering};

use sha2::{Digest as _, Sha256};

use super::plan::split;
use super::*;
use crate::manifest::{self, FileHash};
use crate::store::testutil::TempDir;

const ESBUILD: &str = "Resources/runtime/esbuild-wasm-0.1.0";

struct Fixture {
    tmp: TempDir,
    files: BTreeMap<String, FileHash>,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let tmp = TempDir::new(name);
        let mut fixture = Self {
            tmp,
            files: BTreeMap::new(),
        };
        let node = fs::read("/usr/bin/true").unwrap();
        let files: [(&str, &[u8]); 4] = [
            ("Helpers/node", &node),
            ("Resources/runtime/node-LICENSE", b"licence"),
            ("Resources/runtime/esbuild-wasm-0.1.0/package.json", b"{}"),
            ("Resources/runtime/esbuild-wasm-0.1.0/lib/main.js", b"main"),
        ];
        for (path, bytes) in files {
            fixture.write(path, bytes);
            let (sha256, cdhash) = (format!("{:x}", Sha256::digest(bytes)), None);
            fixture
                .files
                .insert(path.to_owned(), FileHash { sha256, cdhash });
        }
        fixture.write("Resources/runtime/bundle-claude-acp.mjs", b"for T115");
        let node = fixture.contents().join("Helpers/node");
        fs::set_permissions(&node, fs::Permissions::from_mode(0o755)).unwrap();
        let cdhash = cupper_sys::file_cdhash(&node).unwrap().to_string();
        fixture.files.get_mut("Helpers/node").unwrap().cdhash = Some(cdhash);
        fixture
    }

    fn contents(&self) -> PathBuf {
        self.tmp.path().join("Cupper.app/Contents")
    }

    fn write(&self, path: &str, bytes: &[u8]) {
        let path = self.contents().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn runtime(&self) -> PathBuf {
        self.tmp.path().join("data/runtime")
    }

    fn plans(&self) -> Vec<ComponentPlan> {
        split("1.0.0", "0.1.0", &self.files)
    }

    /// Runs an installer; each component's statuses in order, from
    /// `Missing`.
    fn run_with(&self, bundle: Box<dyn Bundle>, in_use: &[(Component, &str)]) -> Histories {
        let in_use = in_use.iter().map(|(c, v)| (*c, (*v).to_owned()));
        let installer = Installer::new(self.plans(), bundle, self.runtime(), in_use.collect());
        let mut histories = histories(&[Status::Missing], &[Status::Missing]);
        installer.run(|statuses| {
            for (component, status) in statuses {
                let history = histories.get_mut(component).unwrap();
                if history.last() != Some(status) {
                    history.push(*status);
                }
            }
        });
        histories
    }

    fn run(&self) -> Histories {
        self.run_with(Box::new(BundleDir(self.contents())), &[])
    }

    /// The names in a component's directory.
    fn listing(&self, component: Component) -> Vec<String> {
        let dir = fs::read_dir(self.runtime().join(component.name())).unwrap();
        let mut names: Vec<_> = dir
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }
}

type Histories = BTreeMap<Component, Vec<Status>>;

fn histories(node: &[Status], esbuild: &[Status]) -> Histories {
    BTreeMap::from([
        (Component::Node, node.to_vec()),
        (Component::EsbuildWasm, esbuild.to_vec()),
    ])
}

const INSTALLED: &[Status] = &[Status::Missing, Status::Installing, Status::Ready];
const BUNDLE_CHANGED: &[Status] = &[
    Status::Missing,
    Status::Installing,
    Status::Failed(Reason::BundleChanged),
];

#[test]
fn it_runtime_install_copies_and_commits_each_component() {
    let fixture = Fixture::new("install");
    assert_eq!(fixture.run(), histories(INSTALLED, INSTALLED));
    let node = fixture.runtime().join("node/1.0.0");
    assert_eq!(
        fs::read(node.join("node")).unwrap(),
        fs::read("/usr/bin/true").unwrap()
    );
    let mode = |path: PathBuf| fs::metadata(path).unwrap().mode() & 0o777;
    assert_eq!(mode(node.join("node")), 0o500);
    assert_eq!(mode(node.join("LICENSE")), 0o400);
    let esbuild = fixture.runtime().join("esbuild-wasm/0.1.0");
    assert_eq!(fs::read(esbuild.join("lib/main.js")).unwrap(), b"main");
    assert_eq!(fixture.listing(Component::Node), ["1.0.0"]);
    // `node`, `LICENSE` and `.installed.json`.
    assert_eq!(fs::read_dir(&node).unwrap().count(), 3);
}

#[test]
fn it_runtime_install_bundled_file_unlike_the_manifest_fails_as_bundle_changed() {
    let fixture = Fixture::new("hash");
    fixture.write(&format!("{ESBUILD}/lib/main.js"), b"changed");
    assert_eq!(fixture.run(), histories(INSTALLED, BUNDLE_CHANGED));
    assert!(fixture.listing(Component::EsbuildWasm).is_empty());

    let mut fixture = Fixture::new("cdhash");
    let node = fixture.files.get_mut("Helpers/node").unwrap();
    node.cdhash = Some("00".repeat(20));
    assert_eq!(fixture.run(), histories(BUNDLE_CHANGED, INSTALLED));
    assert!(fixture.listing(Component::Node).is_empty());
}

#[test]
fn it_runtime_install_killed_before_commit_leaves_nothing_in_use() {
    let fixture = Fixture::new("killed");
    let leftovers = ["node/.staging-1.0.0/node", "esbuild-wasm/.discard-0.0.9/x"];
    for leftover in leftovers.map(|path| fixture.runtime().join(path)) {
        fs::create_dir_all(leftover.parent().unwrap()).unwrap();
        fs::write(leftover, b"half").unwrap();
    }
    assert_eq!(fixture.run(), histories(INSTALLED, INSTALLED));
    assert_eq!(fixture.listing(Component::Node), ["1.0.0"]);
    assert_eq!(fixture.listing(Component::EsbuildWasm), ["0.1.0"]);
}

#[test]
fn it_runtime_install_tampered_after_install_is_moved_aside_and_redone() {
    let fixture = Fixture::new("tampered");
    fixture.run();
    let main = fixture.runtime().join("esbuild-wasm/0.1.0/lib/main.js");
    fs::set_permissions(&main, fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(&main, b"tampered").unwrap();
    fs::remove_file(fixture.runtime().join("node/1.0.0/.installed.json")).unwrap();
    assert_eq!(fixture.run(), histories(INSTALLED, INSTALLED));
    assert_eq!(fs::read(&main).unwrap(), b"main");
    assert_eq!(fixture.listing(Component::Node), ["1.0.0"]);
}

#[test]
fn it_runtime_install_repeated_run_keeps_the_install() {
    let fixture = Fixture::new("repeated");
    fixture.run();
    let node = fixture.runtime().join("node/1.0.0/node");
    let inode = fs::metadata(&node).unwrap().ino();
    let ready = &[Status::Missing, Status::Ready];
    assert_eq!(fixture.run(), histories(ready, ready));
    assert_eq!(fs::metadata(&node).unwrap().ino(), inode);
}

/// Swaps in the next version of the app just before the second file opens.
struct Replaced {
    contents: PathBuf,
    next: PathBuf,
    opened: AtomicUsize,
}

impl Bundle for Replaced {
    fn open(&self, path: &str) -> io::Result<File> {
        if self.opened.fetch_add(1, Ordering::SeqCst) == 1 {
            fs::rename(&self.contents, self.contents.with_extension("old"))?;
            fs::rename(&self.next, &self.contents)?;
        }
        File::open(self.contents.join(path))
    }
}

#[test]
fn it_runtime_install_bundle_replaced_during_the_copy_fails_as_bundle_changed() {
    let fixture = Fixture::new("replaced");
    let next = fixture.tmp.path().join("next");
    for path in fixture.files.keys() {
        let mut bytes = fs::read(fixture.contents().join(path)).unwrap();
        bytes.push(b'\n');
        fs::create_dir_all(next.join(path).parent().unwrap()).unwrap();
        fs::write(next.join(path), bytes).unwrap();
    }
    let contents = fixture.contents();
    let bundle = Replaced {
        contents,
        next,
        opened: AtomicUsize::new(0),
    };
    let ran = fixture.run_with(Box::new(bundle), &[]);
    assert_eq!(ran, histories(BUNDLE_CHANGED, BUNDLE_CHANGED));
    assert!(fixture.listing(Component::Node).is_empty());
}

#[test]
fn it_runtime_install_unwritable_runtime_dir_fails_as_disk() {
    let fixture = Fixture::new("disk");
    fs::create_dir_all(fixture.runtime().parent().unwrap()).unwrap();
    fs::write(fixture.runtime(), b"not a directory").unwrap();
    let disk = &[Status::Missing, Status::Failed(Reason::Disk)];
    assert_eq!(fixture.run(), histories(disk, disk));
}

#[test]
fn it_runtime_install_recycles_the_versions_nothing_uses() {
    let fixture = Fixture::new("recycle");
    for old in ["node/0.9.0", "node/0.8.0", "esbuild-wasm/0.0.1"] {
        fs::create_dir_all(fixture.runtime().join(old).join("lib")).unwrap();
    }
    let in_use = [(Component::Node, "0.8.0")];
    fixture.run_with(Box::new(BundleDir(fixture.contents())), &in_use);
    assert_eq!(fixture.listing(Component::Node), ["0.8.0", "1.0.0"]);
    assert_eq!(fixture.listing(Component::EsbuildWasm), ["0.1.0"]);
}

#[test]
fn it_runtime_install_self_test_checks_the_bundle_and_reports_the_runtime() {
    let mut fixture = Fixture::new("self-test");
    let bundle = BundleDir(fixture.contents());
    let report = |f: &Fixture| self_test::check(&f.files, &bundle, &f.plans(), &f.runtime());
    let runtime = |report: &self_test::Report| report.runtime.values().copied().collect::<Vec<_>>();
    let fresh = report(&fixture);
    assert!(
        fresh.ok && fresh.problems.is_empty() && fresh.files == 4,
        "{fresh:?}"
    );
    assert_eq!(runtime(&fresh), ["missing", "missing"]);
    fixture.run();
    fs::remove_file(fixture.runtime().join("node/1.0.0/LICENSE")).unwrap();
    let installed = report(&fixture);
    assert!(installed.ok, "the runtime does not decide: {installed:?}");
    assert_eq!(runtime(&installed), ["ready", "broken"]);

    fs::remove_file(fixture.contents().join("Resources/runtime/node-LICENSE")).unwrap();
    fixture.write(&format!("{ESBUILD}/package.json"), b"[]");
    fixture.files.get_mut("Helpers/node").unwrap().cdhash = Some("00".repeat(20));
    let broken = report(&fixture);
    let problems: Vec<_> = broken
        .problems
        .iter()
        .map(|p| (p.file.as_str(), p.problem))
        .collect();
    let package = format!("{ESBUILD}/package.json");
    let want = [
        ("Helpers/node", "cdhash"),
        (package.as_str(), "sha256"),
        ("Resources/runtime/node-LICENSE", "missing"),
    ];
    assert!(!broken.ok);
    assert_eq!(problems, want);
}

#[test]
fn plans_of_the_embedded_manifest_cover_node_and_esbuild_wasm() {
    let manifest = manifest::embedded();
    let plans = plans(manifest);
    let node = &plans[0];
    assert_eq!(node.version, manifest.node.version);
    assert_eq!(node.files.keys().collect::<Vec<_>>(), ["LICENSE", "node"]);
    assert!(node.files["node"].hash.cdhash.is_some());
    let esbuild = &plans[1];
    assert_eq!(esbuild.version, manifest.esbuild_wasm.version);
    assert!(esbuild.files.contains_key("esbuild.wasm"));
    assert!(esbuild.files.contains_key("lib/main.js"));
    // The rest belongs to T111 and T115.
    let rest = manifest.files.keys().filter(|path| {
        ["codex-acp-", "locks/", "bundle-claude-acp.mjs"]
            .iter()
            .any(|p| path.starts_with(&format!("Resources/runtime/{p}")))
    });
    let planned = plans.iter().map(|plan| plan.files.len()).sum::<usize>();
    assert_eq!(planned + rest.count(), manifest.files.len());
}
