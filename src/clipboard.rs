use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use wl_clipboard_rs::{copy, paste};

#[derive(Serialize, Deserialize)]
pub struct ClipboardItem {
    mime: String,
    bytes: Vec<u8>,
}

pub struct PasteClipboard {
    previous: Vec<ClipboardItem>,
    marker: String,
}

fn read_clipboard() -> Result<Vec<ClipboardItem>> {
    let types = match paste::get_mime_types_ordered(paste::ClipboardType::Regular, paste::Seat::Unspecified) {
        Ok(types) => types,
        Err(paste::Error::ClipboardEmpty) => return Ok(Vec::new()),
        Err(error) => return Err(error).context("Could not read the Wayland clipboard; clipboard-preserving paste requires data-control support."),
    };
    types
        .into_iter()
        .map(|mime| {
            let (mut pipe, _) = paste::get_contents(
                paste::ClipboardType::Regular,
                paste::Seat::Unspecified,
                paste::MimeType::Specific(&mime),
            )?;
            let mut bytes = Vec::new();
            pipe.read_to_end(&mut bytes)
                .with_context(|| format!("Could not preserve clipboard format {mime}."))?;
            Ok(ClipboardItem { mime, bytes })
        })
        .collect()
}

async fn publish(items: Vec<ClipboardItem>) -> Result<()> {
    if items.is_empty() {
        return tokio::task::spawn_blocking(|| {
            copy::clear(copy::ClipboardType::Regular, copy::Seat::All)
        })
        .await?
        .context("Could not restore the empty clipboard.");
    }
    // A separate owner keeps restored data available after the MCP session exits.
    let mut child = tokio::process::Command::new(std::env::current_exe()?)
        .arg("clipboard-owner")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let mut input = child.stdin.take().unwrap();
    input.write_all(&serde_json::to_vec(&items)?).await?;
    drop(input);
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut ready = String::new();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        output.read_line(&mut ready),
    )
    .await;
    if !matches!(result, Ok(Ok(_))) || ready.trim() != "ready" {
        let _ = child.kill().await;
        anyhow::bail!("Wayland clipboard owner failed to publish the requested formats.");
    }
    // Reap the owner when another application takes the clipboard.
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    Ok(())
}

impl PasteClipboard {
    pub async fn prepare(text: &str, html: Option<&str>) -> Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let previous = tokio::task::spawn_blocking(read_clipboard).await??;
        let marker = format!(
            "application/x-computer-use-paste-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let mut items = vec![
            ClipboardItem {
                mime: "text/plain;charset=utf-8".into(),
                bytes: text.as_bytes().to_vec(),
            },
            ClipboardItem {
                mime: "text/plain".into(),
                bytes: text.as_bytes().to_vec(),
            },
            ClipboardItem {
                mime: marker.clone(),
                bytes: vec![],
            },
        ];
        if let Some(html) = html {
            items.insert(
                0,
                ClipboardItem {
                    mime: "text/html".into(),
                    bytes: html.as_bytes().to_vec(),
                },
            );
        }
        publish(items).await?;
        Ok(Self { previous, marker })
    }

    pub async fn restore(self) -> Result<bool> {
        let types = match tokio::task::spawn_blocking(|| {
            paste::get_mime_types(paste::ClipboardType::Regular, paste::Seat::Unspecified)
        })
        .await?
        {
            Ok(types) => types,
            Err(paste::Error::ClipboardEmpty) => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        if !types.contains(&self.marker) {
            return Ok(false);
        }
        publish(self.previous).await?;
        Ok(true)
    }
}

pub fn serve_owner() -> Result<()> {
    let items: Vec<ClipboardItem> = serde_json::from_reader(std::io::stdin().lock())?;
    let mut options = copy::Options::new();
    options
        .foreground(true)
        .omit_additional_text_mime_types(true);
    let prepared = options.prepare_copy_multi(
        items
            .into_iter()
            .map(|item| copy::MimeSource {
                source: copy::Source::Bytes(item.bytes.into_boxed_slice()),
                mime_type: copy::MimeType::Specific(item.mime),
            })
            .collect(),
    )?;
    println!("ready");
    std::io::stdout().flush()?;
    prepared.serve()?;
    Ok(())
}
