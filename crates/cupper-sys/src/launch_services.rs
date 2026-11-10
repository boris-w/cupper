//! An app bundle's LaunchServices registration (design 5.2.9, rule 1). The
//! system registers an app put in `/Applications`, but not the host bundle
//! nested in it, and the notification service turns away an app that
//! LaunchServices does not have (T101). So the desktop registers the host
//! before it starts it, and the host checks again as it starts (design 5.7.2).

use std::fs;
use std::path::{Path, PathBuf};

use core_foundation::base::{Boolean, OSStatus, TCFType};
use core_foundation::url::{CFURL, CFURLRef};
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSBundle, NSString};

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
