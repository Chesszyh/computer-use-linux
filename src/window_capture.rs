use crate::screenshot::RawScreenshotCapture;
use anyhow::{bail, Context, Result};
use image::{DynamicImage, RgbaImage};
use std::{
    fs::File,
    io::Cursor,
    os::fd::{AsFd, FromRawFd},
    os::unix::fs::FileExt,
};
use wayland_client::{
    delegate_noop,
    protocol::{wl_buffer, wl_registry, wl_shm, wl_shm_pool},
    Connection, Dispatch, QueueHandle, WEnum,
};
use wayland_protocols_hyprland::toplevel_export::v1::client::{
    hyprland_toplevel_export_frame_v1 as frame, hyprland_toplevel_export_manager_v1 as manager,
};

#[derive(Default)]
struct Capture {
    manager: Option<manager::HyprlandToplevelExportManagerV1>,
    shm: Option<wl_shm::WlShm>,
    file: Option<File>,
    buffer: Option<wl_buffer::WlBuffer>,
    format: Option<wl_shm::Format>,
    width: u32,
    height: u32,
    stride: u32,
    inverted: bool,
    ready: bool,
    error: Option<String>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for Capture {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_shm" => state.shm = Some(registry.bind(name, 1, qh, ())),
                "hyprland_toplevel_export_manager_v1" => {
                    state.manager = Some(registry.bind(name, version.min(2), qh, ()))
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<frame::HyprlandToplevelExportFrameV1, ()> for Capture {
    fn event(
        state: &mut Self,
        proxy: &frame::HyprlandToplevelExportFrameV1,
        event: frame::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            frame::Event::Buffer {
                format,
                width,
                height,
                stride,
            } => {
                state.width = width;
                state.height = height;
                state.stride = stride;
                state.format = match format {
                    WEnum::Value(format) => Some(format),
                    _ => None,
                };
            }
            frame::Event::BufferDone => {
                if let Err(error) = state.allocate(proxy, qh) {
                    state.error = Some(error.to_string());
                }
            }
            frame::Event::Flags {
                flags: WEnum::Value(flags),
            } => state.inverted = flags.contains(frame::Flags::YInvert),
            frame::Event::Ready { .. } => state.ready = true,
            frame::Event::Failed => state.error = Some(
                "Hyprland could not capture this window; it may have closed or stopped rendering."
                    .into(),
            ),
            _ => {}
        }
    }
}

impl Capture {
    fn allocate(
        &mut self,
        frame: &frame::HyprlandToplevelExportFrameV1,
        qh: &QueueHandle<Self>,
    ) -> Result<()> {
        let format = self
            .format
            .context("Window capture did not provide a shared-memory format.")?;
        anyhow::ensure!(
            matches!(
                format,
                wl_shm::Format::Argb8888
                    | wl_shm::Format::Xrgb8888
                    | wl_shm::Format::Abgr8888
                    | wl_shm::Format::Xbgr8888
            ),
            "Unsupported window capture pixel format: {format:?}"
        );
        let size = self
            .stride
            .checked_mul(self.height)
            .context("Window capture buffer is too large.")?;
        anyhow::ensure!(
            self.width > 0
                && self.height > 0
                && self.stride >= self.width * 4
                && size <= i32::MAX as u32,
            "Invalid window capture dimensions."
        );
        let fd = unsafe { libc::memfd_create(c"computer-use-window".as_ptr(), libc::MFD_CLOEXEC) };
        anyhow::ensure!(
            fd >= 0,
            "Could not allocate window capture memory: {}",
            std::io::Error::last_os_error()
        );
        let file = unsafe { File::from_raw_fd(fd) };
        file.set_len(size as u64)?;
        let pool = self
            .shm
            .as_ref()
            .context("wl_shm is unavailable.")?
            .create_pool(file.as_fd(), size as i32, qh, ());
        let buffer = pool.create_buffer(
            0,
            self.width as i32,
            self.height as i32,
            self.stride as i32,
            format,
            qh,
            (),
        );
        frame.copy(&buffer, 1);
        pool.destroy();
        self.file = Some(file);
        self.buffer = Some(buffer);
        Ok(())
    }
}

delegate_noop!(Capture: ignore manager::HyprlandToplevelExportManagerV1);
delegate_noop!(Capture: ignore wl_shm::WlShm);
delegate_noop!(Capture: ignore wl_shm_pool::WlShmPool);
delegate_noop!(Capture: ignore wl_buffer::WlBuffer);

pub fn capture_png(window_id: u64) -> Result<Vec<u8>> {
    let connection =
        Connection::connect_to_env().context("Could not connect to the Wayland compositor.")?;
    let mut queue = connection.new_event_queue();
    let qh = queue.handle();
    connection.display().get_registry(&qh, ());
    let mut state = Capture::default();
    queue.roundtrip(&mut state)?;
    let manager = state
        .manager
        .as_ref()
        .context("This compositor does not expose Hyprland window capture.")?;
    // The protocol represents hyprctl addresses by their low 32 bits.
    let frame = manager.capture_toplevel(0, window_id as u32, &qh, ());
    while !state.ready && state.error.is_none() {
        queue.blocking_dispatch(&mut state)?;
    }
    frame.destroy();
    if let Some(error) = state.error {
        bail!(error);
    }
    let mut bytes = vec![0; (state.stride as usize) * state.height as usize];
    state
        .file
        .context("No capture buffer was created.")?
        .read_exact_at(&mut bytes, 0)?;
    let rgba = convert_pixels(
        &bytes,
        state.width,
        state.height,
        state.stride,
        state.format.unwrap(),
        state.inverted,
    );
    let image =
        RgbaImage::from_raw(state.width, state.height, rgba).context("Invalid captured pixels.")?;
    let mut output = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image).write_to(&mut output, image::ImageFormat::Png)?;
    Ok(output.into_inner())
}

fn convert_pixels(
    bytes: &[u8],
    width: u32,
    height: u32,
    stride: u32,
    format: wl_shm::Format,
    inverted: bool,
) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        let row = if inverted { height - 1 - y } else { y };
        for x in 0..width {
            let offset = (row * stride + x * 4) as usize;
            let pixel = u32::from_ne_bytes(bytes[offset..offset + 4].try_into().unwrap());
            let (red, blue) =
                if matches!(format, wl_shm::Format::Abgr8888 | wl_shm::Format::Xbgr8888) {
                    (pixel as u8, (pixel >> 16) as u8)
                } else {
                    ((pixel >> 16) as u8, pixel as u8)
                };
            let alpha = if matches!(format, wl_shm::Format::Xrgb8888 | wl_shm::Format::Xbgr8888) {
                255
            } else {
                (pixel >> 24) as u8
            };
            // Wayland ARGB is premultiplied; PNG stores straight-alpha channels.
            let straight = |channel: u8| {
                if alpha == 0 {
                    0
                } else {
                    ((u16::from(channel) * 255 + u16::from(alpha) / 2) / u16::from(alpha)).min(255)
                        as u8
                }
            };
            rgba.extend_from_slice(&[
                straight(red),
                straight((pixel >> 8) as u8),
                straight(blue),
                alpha,
            ]);
        }
    }
    rgba
}

pub async fn capture_window(window_id: u64) -> Result<RawScreenshotCapture> {
    let mut command = tokio::process::Command::new(std::env::current_exe()?);
    command.args(["capture-window", &window_id.to_string()]);
    let output = crate::command_runner::output_with_timeout(
        command,
        "capture Hyprland window",
        std::time::Duration::from_secs(10),
    )
    .await?;
    anyhow::ensure!(
        output.status.success(),
        "Window capture failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let image = image::load_from_memory(&output.stdout)?;
    Ok(RawScreenshotCapture {
        mime_type: "image/png".into(),
        bytes: output.stdout,
        source: "hyprland-toplevel-export".into(),
        width: image.width(),
        height: image.height(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unpremultiplies_translucent_window_pixels() {
        assert_eq!(
            convert_pixels(
                &0x80402010u32.to_ne_bytes(),
                1,
                1,
                4,
                wl_shm::Format::Argb8888,
                false
            ),
            vec![128, 64, 32, 128]
        );
    }

    #[test]
    fn converts_stride_inversion_and_channel_order() {
        let bytes: Vec<_> = [0x00112233u32, 0, 0x00445566, 0]
            .into_iter()
            .flat_map(u32::to_ne_bytes)
            .collect();
        assert_eq!(
            convert_pixels(&bytes, 1, 2, 8, wl_shm::Format::Xrgb8888, true),
            vec![0x44, 0x55, 0x66, 255, 0x11, 0x22, 0x33, 255]
        );
        assert_eq!(
            convert_pixels(&bytes, 1, 1, 8, wl_shm::Format::Xbgr8888, false),
            vec![0x33, 0x22, 0x11, 255]
        );
    }
}
