pub mod capture;
pub mod input;
pub mod overlay;

pub use capture::{
    capture_region, capture_screen, capture_window, list_windows, ScreenshotResult, WindowInfo,
};
pub use input::{ComputerAction, InputController};
pub use overlay::{CoordinateTransform, RegionBounds};
