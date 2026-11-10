//! An app bundle's LaunchServices registration (design 5.2.9, rule 1). The
//! system registers an app put in `/Applications`, but not the host bundle
//! nested in it, and the notification service turns away an app that
//! LaunchServices does not have (T101). So the desktop registers the host
//! before it starts it, and the host checks again as it starts (design 5.7.2).
//!
//! After a click on a notification the host brings the app forward, or
//! starts it with the one URL that says where to go (design 5.2.9).

use std::fs;
use std::path::{Path, PathBuf};

use block2::RcBlock;
use core_foundation::base::{Boolean, OSStatus, TCFType};
use core_foundation::url::{CFURL, CFURLRef};
use objc2_app_kit::{NSRunningApplication, NSWorkspace, NSWorkspaceOpenConfiguration};
use objc2_foundation::{NSArray, NSBundle, NSError, NSString, NSURL};

use crate::notify::Once;
use crate::{Error, Result};

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn LSRegisterURL(url: CFURLRef, update: Boolean) -> OSStatus;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Registration {
    /// LaunchServices had the app at that path already.
    Already,
    /// It does now.
    Registered,
}

/// Where LaunchServices has apps of `bundle_id`.
pub fn registered_apps(bundle_id: &str) -> Vec<PathBuf> {
    let workspace = NSWorkspace::sharedWorkspace();
    let urls = workspace.URLsForApplicationsWithBundleIdentifier(&NSString::from_str(bundle_id));
    let paths = urls.iter().filter_map(|url| url.path());
    paths.map(|path| PathBuf::from(path.to_string())).collect()
}

/// The `CFBundleIdentifier` of the bundle at `bundle`.
pub fn bundle_identifier(bundle: &Path) -> Option<String> {
    let path = NSString::from_str(bundle.to_str()?);
    let id = NSBundle::bundleWithPath(&path)?.bundleIdentifier()?;
    Some(id.to_string())
}

/// Registers the app at `bundle`, whose identifier is `bundle_id`, unless
/// LaunchServices has it at that path already; another copy of the same
/// identifier elsewhere does not count.
pub fn ensure_registered(bundle: &Path, bundle_id: &str) -> Result<Registration> {
    let os = |source| Error::Os {
        call: "realpath",
        source,
    };
    let bundle = fs::canonicalize(bundle).map_err(os)?;
    let same = |path: &PathBuf| fs::canonicalize(path).is_ok_and(|path| path == bundle);
    if registered_apps(bundle_id).iter().any(same) {
        return Ok(Registration::Already);
    }
    let url = CFURL::from_path(&bundle, true).ok_or(Error::InvalidArgument("bundle path"))?;
    // SAFETY: a live CFURL; `false`: register, not update an unchanged entry.
    let status = unsafe { LSRegisterURL(url.as_concrete_TypeRef(), 0) };
    if status != 0 {
        let call = "LSRegisterURL";
        return Err(Error::LaunchServices { call, status });
    }
    Ok(Registration::Registered)
}

/// Hands `url` to the app at `app` and no other, starting it if it does not
/// run, and brings it forward; whether that worked, to `done`. Naming the app
/// keeps the URL from any other app that claims its scheme (security #7);
/// the app gets it before it finishes launching (T101).
pub fn open_url_with_app(url: &str, app: &Path, done: impl FnOnce(Result<()>) + Send + 'static) {
    let Some(url) = NSURL::URLWithString(&NSString::from_str(url)) else {
        return done(Err(Error::InvalidArgument("url")));
    };
    let urls = NSArray::from_retained_slice(&[url]);
    NSWorkspace::sharedWorkspace().openURLs_withApplicationAtURL_configuration_completionHandler(
        &urls,
        &file_url(app),
        &NSWorkspaceOpenConfiguration::configuration(),
        Some(&opened("openURLs:withApplicationAtURL:", done)),
    );
}

/// Brings the app at `app` forward, starting it if it does not run; whether
/// that worked, to `done`.
pub fn open_app(app: &Path, done: impl FnOnce(Result<()>) + Send + 'static) {
    NSWorkspace::sharedWorkspace().openApplicationAtURL_configuration_completionHandler(
        &file_url(app),
        &NSWorkspaceOpenConfiguration::configuration(),
        Some(&opened("openApplicationAtURL:", done)),
    );
}

fn file_url(path: &Path) -> objc2::rc::Retained<NSURL> {
    NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()))
}

/// NSWorkspace's completion handler, which may run on any queue.
fn opened(
    call: &'static str,
    done: impl FnOnce(Result<()>) + Send + 'static,
) -> RcBlock<dyn Fn(*mut NSRunningApplication, *mut NSError)> {
    let done = Once::new(done);
    RcBlock::new(
        move |_app: *mut NSRunningApplication, error: *mut NSError| {
            // SAFETY: null, or an error that lives while the block runs.
            let result = match unsafe { error.as_ref() } {
                None => Ok(()),
                Some(error) => Err(Error::LaunchServices {
                    call,
                    status: error.code() as i32,
                }),
            };
            if let Some(done) = done.take() {
                done(result);
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    #[test]
    fn launch_services_has_no_app_of_an_unknown_identifier() {
        assert_eq!(
            registered_apps("dev.cupper.test.nobody-has-this"),
            [] as [PathBuf; 0]
        );
    }

    #[test]
    fn launch_services_bundle_identifier_comes_from_info_plist() {
        let dir = TempDir::new();
        let app = dir.path().join("Probe.app");
        fs::create_dir_all(app.join("Contents")).unwrap();
        let plist = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>CFBundleIdentifier</key><string>dev.cupper.test.probe</string></dict></plist>"#;
        fs::write(app.join("Contents/Info.plist"), plist).unwrap();
        assert_eq!(
            bundle_identifier(&app).as_deref(),
            Some("dev.cupper.test.probe")
        );
        assert_eq!(bundle_identifier(&dir.path().join("Missing.app")), None);
    }
}
