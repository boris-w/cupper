//! Resolving a found CLI to the native binary an adapter gets (design 5.6.6,
//! ADR-08, Q19). An adapter cannot take an npm entry or a JavaScript file: a
//! GUI process has no `node` on its PATH. Install layouts are data, one
//! [`Layout`] per agent; the task notes of T054 record how each was checked.

use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use cupper_protocol::index::CliInstall;

/// Where one agent's CLI gets installed.
#[derive(Debug)]
pub struct Layout {
    /// The program's name on `PATH`.
    pub program: &'static str,
    /// Where installers put it, looked at after the login shell's `PATH`;
    /// `~/` stands for the home directory.
    pub common: &'static [&'static str],
    pub npm: &'static [NpmPackage],
}

/// An npm package of the CLI.
#[derive(Debug)]
pub struct NpmPackage {
    /// The package directory, as it ends a path.
    pub dir: &'static str,
    /// Its `bin` entries, relative to the package directory.
    pub entries: &'static [&'static str],
    /// Where the platform binary may be, relative to the package directory.
    pub natives: &'static [&'static str],
}

pub const CLAUDE: Layout = Layout {
    program: "claude",
    common: &[
        // The Homebrew cask links `Caskroom/claude-code*/<version>/claude`;
        // npm under Homebrew's node puts its entry here too.
        "/opt/homebrew/bin/claude",
        // The native installer links `~/.local/share/claude/versions/<version>`
        // (official docs; not checked on a machine).
        "~/.local/bin/claude",
        // npm with the official Node installer's prefix, and with the prefix
        // npm's docs suggest.
        "/usr/local/bin/claude",
        "~/.npm-global/bin/claude",
    ],
    npm: &[NpmPackage {
        dir: "@anthropic-ai/claude-code",
        // From 2.1.120 a stub that postinstall replaces with the platform
        // binary; up to 2.1.100 `cli.js`, JavaScript only, whose `vendor/`
        // Mach-O files are tools like `rg`.
        entries: &["bin/claude.exe", "cli.js"],
        // The platform package: nested, or beside the package when hoisted
        // and in pnpm's store.
        natives: &[
            "node_modules/@anthropic-ai/claude-code-darwin-arm64/claude",
            "../claude-code-darwin-arm64/claude",
        ],
    }],
};

/// What a found CLI resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// The native binary, as a realpath.
    Native {
        binary: PathBuf,
        install: CliInstall,
    },
    /// Installed without a native binary: a JavaScript entry, an npm stub
    /// whose platform package is missing, a shell script (pnpm).
    Unsupported,
}

/// Resolves `found`, or `None` when it is no executable file. `real` maps a
/// path to its realpath, or refuses it; it is also applied to the platform
/// binaries of an npm package.
pub fn resolve(
    found: &Path,
    layout: &Layout,
    real: impl Fn(&Path) -> Option<PathBuf>,
) -> Option<Resolved> {
    let path = real(found).filter(|path| is_executable(path))?;
    if is_mach_o(&path) {
        let install = CliInstall::Native;
        return Some(Resolved::Native {
            binary: path,
            install,
        });
    }
    for package in layout.npm {
        for entry in package.entries {
            let Some(dir) = package_dir(&path, package.dir, entry) else {
                continue;
            };
            let native = package
                .natives
                .iter()
                .filter_map(|native| real(&dir.join(native)))
                .find(|binary| is_executable(binary) && is_mach_o(binary));
            if let Some(binary) = native {
                let install = CliInstall::NpmShim;
                return Some(Resolved::Native { binary, install });
            }
        }
    }
    Some(Resolved::Unsupported)
}

/// The package directory, when `path` is `entry` in the package `dir`.
fn package_dir(path: &Path, dir: &str, entry: &str) -> Option<PathBuf> {
    if !path.ends_with(Path::new(dir).join(entry)) {
        return None;
    }
    let mut package = path.to_path_buf();
    for _ in Path::new(entry).components() {
        package.pop();
    }
    Some(package)
}

pub(super) fn is_executable(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// A Mach-O file, 32 or 64 bit, or a universal one, by its magic number.
fn is_mach_o(path: &Path) -> bool {
    const MAGIC: [[u8; 4]; 4] = [
        [0xcf, 0xfa, 0xed, 0xfe],
        [0xce, 0xfa, 0xed, 0xfe],
        [0xca, 0xfe, 0xba, 0xbe],
        [0xca, 0xfe, 0xba, 0xbf],
    ];
    let mut head = [0; 4];
    File::open(path)
        .and_then(|mut file| file.read_exact(&mut head))
        .is_ok()
        && MAGIC.contains(&head)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::locate::testkit::Tree;

    #[test]
    fn unit_cli_resolver_claude_install_layouts() {
        // Below an npm prefix: the package, its platform package, pnpm's store.
        let pkg = "lib/node_modules/@anthropic-ai/claude-code";
        let arm64 = "node_modules/@anthropic-ai/claude-code-darwin-arm64";
        let store = "pnpm/.pnpm/cc@1/node_modules/@anthropic-ai";
        let store_arm64 = "pnpm/.pnpm/cc-arm64@1/node_modules/@anthropic-ai";
        let tree = Tree::new(
            "resolver",
            &format!(
                "
                brew/Caskroom/claude-code/2.1.296/claude      macho
                brew/bin/claude                               -> ../Caskroom/claude-code/2.1.296/claude
                home/.local/share/claude/versions/2.1.296     macho
                home/.local/bin/claude                        -> ../share/claude/versions/2.1.296
                home/bin/claude                               -> ../.local/bin/claude
                npm/{pkg}/bin/claude.exe                      macho
                npm/bin/claude                                -> ../{pkg}/bin/claude.exe
                nested/{pkg}/bin/claude.exe                   exec stub
                nested/{pkg}/{arm64}/claude                   macho
                {store}/claude-code/bin/claude.exe            exec stub
                {store_arm64}/claude-code-darwin-arm64/claude macho
                {store}/claude-code-darwin-arm64              -> ../../../cc-arm64@1/{arm64}
                bare/{pkg}/bin/claude.exe                     exec stub
                old/{pkg}/cli.js                              exec #!/usr/bin/env node
                old/{pkg}/vendor/ripgrep/arm64-darwin/rg      macho
                old/bin/claude                                -> ../{pkg}/cli.js
                home/Library/pnpm/claude                      exec #!/bin/sh
                plain/claude                                  plain not executable
                dir/claude                                    dir
                "
            ),
        );
        // Homebrew cask; native installer through a chain of links; npm
        // after postinstall (the entry is the binary), without it (nested
        // and in pnpm's store), and without the platform package; JavaScript
        // only, with `rg` beside it; pnpm's global bin, a shell script.
        let cases = format!(
            "
            brew/bin/claude                    native brew/Caskroom/claude-code/2.1.296/claude
            home/bin/claude                    native home/.local/share/claude/versions/2.1.296
            npm/bin/claude                     native npm/{pkg}/bin/claude.exe
            nested/{pkg}/bin/claude.exe        npm nested/{pkg}/{arm64}/claude
            {store}/claude-code/bin/claude.exe npm {store_arm64}/claude-code-darwin-arm64/claude
            bare/{pkg}/bin/claude.exe          unsupported
            old/bin/claude                     unsupported
            home/Library/pnpm/claude           unsupported
            plain/claude                       none
            dir/claude                         none
            missing/claude                     none
            "
        );
        let real = |path: &Path| fs::canonicalize(path).ok();
        for case in cases.lines().map(str::split_whitespace) {
            let [found, kind, binary @ ..] = &case.collect::<Vec<_>>()[..] else {
                continue;
            };
            let want = match (*kind, binary) {
                ("unsupported", _) => Some(Resolved::Unsupported),
                ("none", _) => None,
                (kind, [binary]) => Some(Resolved::Native {
                    binary: tree.path(binary),
                    install: match kind {
                        "npm" => CliInstall::NpmShim,
                        _ => CliInstall::Native,
                    },
                }),
                _ => panic!("bad case {found}"),
            };
            assert_eq!(resolve(&tree.path(found), &CLAUDE, real), want, "{found}");
        }
        // A platform binary that `real` refuses is not taken.
        let refuse_natives = |path: &Path| real(path).filter(|p| !p.ends_with("claude"));
        let found = tree.path(&format!("nested/{pkg}/bin/claude.exe"));
        let resolved = resolve(&found, &CLAUDE, refuse_natives);
        assert_eq!(resolved, Some(Resolved::Unsupported));
    }
}
