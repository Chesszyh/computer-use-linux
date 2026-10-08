mod abs_pointer;
mod accessibility_guard;
mod applications;
#[path = "atspi_tree.rs"]
mod atspi_tree_impl;
mod cli;
mod clipboard;
mod command_runner;
mod cosmic_helper;
#[path = "diagnostics.rs"]
mod diagnostics_impl;
mod gnome_extension;
mod identity;
pub mod indicator;
mod keyboard_keymap;
mod observations;
mod remote_desktop;
#[path = "screenshot.rs"]
mod screenshot_impl;
mod server;
mod terminal;
mod window_capture;
mod windowing;
mod windows;
mod x11_display;
mod ydotool;

pub mod atspi_tree {
    pub(crate) use crate::atspi_tree_impl::{
        focused_element_summary_in_app, insert_element_text, list_accessible_apps,
        object_ref_owner_pid, perform_action, perform_named_action, probe_focused_element,
        select_element_text, set_element_value, snapshot_accessibility_tree, snapshot_limits,
        AccessibleAppSummary, FocusProbe, FocusedElementSummary, SelectionType, ValueSetInvocation,
        UNKNOWN_ROLE,
    };
    pub use crate::atspi_tree_impl::{
        snapshot_tree, AccessibilityAction, AccessibilityNode, AccessibilityText,
        AccessibilityTextSelection, AccessibilityValue, Bounds,
    };
}

pub mod diagnostics {
    pub use crate::diagnostics_impl::{
        doctor_report, hydrate_session_bus_env, AccessibilityReport, CapabilityMap, Check,
        DoctorReport, InputReport, PlatformReport, PortalReport, PreferredBackends,
        ReadinessReport, ScreenshotCaptureStatus, WindowingReport,
    };
    pub(crate) use crate::diagnostics_impl::{
        setup_accessibility_report, wtype_compatible_wayland_desktop, SetupReport,
    };
}

pub mod screenshot {
    pub(crate) use crate::screenshot_impl::{
        capture_screenshot, prepare_screenshot_payload, ScreenshotCapture, ScreenshotOutputFormat,
        ScreenshotPayloadOptions,
    };
    pub use crate::screenshot_impl::{capture_screenshot_raw, RawScreenshotCapture};
}

#[doc(hidden)]
pub async fn run_cli_from_env() -> anyhow::Result<()> {
    cli::run_from_env().await
}
