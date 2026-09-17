use craft_core::{CraftError, Result};

/// Resolves a user-provided URL into a direct world archive download URL (.zip, .mcworld).
/// Supports direct archive links, MediaFire file pages, Google Drive links, Dropbox links,
/// GitHub releases/blobs, and generic web pages with automatic Cloudflare robot check detection.
pub async fn resolve_map_download_url(url: &str) -> Result<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(CraftError::Other("Map URL cannot be empty.".to_string()));
    }

    let lower = trimmed.to_lowercase();

    // 1. Direct archive URL (.zip, .mcworld, .tar.gz, etc.)
    if lower.ends_with(".zip")
        || lower.ends_with(".mcworld")
        || lower.ends_with(".tar.gz")
        || lower.ends_with(".tgz")
        || lower.contains("media.forgecdn.net/files/")
        || lower.contains("/releases/download/")
    {
        return Ok(trimmed.to_string());
    }

    // 2. Google Drive links:
    // https://drive.google.com/file/d/<id>/view...
    // https://drive.google.com/open?id=<id>
    if lower.contains("drive.google.com") {
        if let Some(id) = extract_google_drive_id(trimmed) {
            return Ok(format!(
                "https://drive.google.com/uc?export=download&id={}&confirm=t",
                id
            ));
        }
    }

    // 3. Dropbox links:
    // https://www.dropbox.com/s/... -> dl=1
    if lower.contains("dropbox.com") {
        let direct = if trimmed.contains("dl=0") {
            trimmed.replace("dl=0", "dl=1")
        } else if trimmed.contains('?') {
            format!("{}&dl=1", trimmed)
        } else {
            format!("{}?dl=1", trimmed)
        };
        return Ok(direct);
    }

    // 4. GitHub repository / raw links
    if lower.contains("github.com") {
        if let Some(raw_url) = resolve_github_url(trimmed) {
            return Ok(raw_url);
        }
    }

    // 5. MediaFire pages
    if lower.contains("mediafire.com") {
        return resolve_mediafire_url(trimmed).await;
    }

    // 6. Generic web page: fetch and inspect HTML
    resolve_generic_web_page(trimmed).await
}

fn extract_google_drive_id(url: &str) -> Option<String> {
    if let Some(pos) = url.find("/file/d/") {
        let rest = &url[pos + 8..];
        let id = rest.split('/').next()?.split('?').next()?;
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }
    if let Some(pos) = url.find("id=") {
        let rest = &url[pos + 3..];
        let id = rest.split('&').next()?.split('#').next()?;
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }
    None
}

fn resolve_github_url(url: &str) -> Option<String> {
    if url.contains("/raw/") && (url.ends_with(".zip") || url.ends_with(".mcworld")) {
        return Some(url.to_string());
    }
    None
}

async fn resolve_mediafire_url(url: &str) -> Result<String> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()
        .map_err(|e| CraftError::Other(e.to_string()))?;

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| CraftError::Download(format!("Failed to reach MediaFire: {}", e)))?;

    let html = resp
        .text()
        .await
        .map_err(|e| CraftError::Download(format!("Failed to read MediaFire page: {}", e)))?;

    if let Some(download_url) = extract_mediafire_download_link(&html) {
        return Ok(download_url);
    }

    Err(CraftError::Other(
        "Could not extract direct download URL from MediaFire page. Please check if the file still exists or paste the direct mirror link."
            .to_string(),
    ))
}

fn extract_mediafire_download_link(html: &str) -> Option<String> {
    if let Some(pos) = html.find("id=\"downloadButton\"") {
        let start_window = pos.saturating_sub(200);
        let end_window = (pos + 300).min(html.len());
        let window = &html[start_window..end_window];
        if let Some(href_pos) = window.find("href=\"") {
            let rest = &window[href_pos + 6..];
            if let Some(quote_pos) = rest.find('"') {
                let link = &rest[..quote_pos];
                if link.starts_with("http") {
                    return Some(link.to_string());
                }
            }
        }
    }

    if let Some(pos) = html.find("aria-label=\"Download file\"") {
        let start_window = pos.saturating_sub(200);
        let end_window = (pos + 300).min(html.len());
        let window = &html[start_window..end_window];
        if let Some(href_pos) = window.find("href=\"") {
            let rest = &window[href_pos + 6..];
            if let Some(quote_pos) = rest.find('"') {
                let link = &rest[..quote_pos];
                if link.starts_with("http") {
                    return Some(link.to_string());
                }
            }
        }
    }

    if let Some(pos) = html.find("https://download") {
        let rest = &html[pos..];
        if let Some(end_pos) = rest.find(['"', '\'', ' ', '<']) {
            let candidate = &rest[..end_pos];
            if candidate.contains(".mediafire.com/") {
                return Some(candidate.to_string());
            }
        }
    }

    None
}

async fn resolve_generic_web_page(url: &str) -> Result<String> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| CraftError::Other(e.to_string()))?;

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| CraftError::Download(format!("Failed to reach '{}': {}", url, e)))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| {
        CraftError::Download(format!("Failed to read content from '{}': {}", url, e))
    })?;

    // Check for Cloudflare Turnstile / Managed Challenge
    if status == reqwest::StatusCode::FORBIDDEN
        || status == reqwest::StatusCode::SERVICE_UNAVAILABLE
        || text.contains("Just a moment...")
        || text.contains("cf-chl-opt")
        || text.contains("challenges.cloudflare.com")
    {
        return Err(CraftError::Other(format!(
            "'{}' is protected by Cloudflare anti-bot verification. Please open the link in your browser, copy the direct download or file mirror link (e.g. MediaFire, Google Drive, or .zip), and paste that link instead.",
            url
        )));
    }

    // Try finding direct archive link inside HTML
    if let Some(link) = extract_archive_link_from_html(&text, url) {
        return Ok(link);
    }

    Err(CraftError::Other(format!(
        "Could not automatically locate a downloadable map archive (.zip/.mcworld) on '{}'. Please provide a direct download link.",
        url
    )))
}

fn extract_archive_link_from_html(html: &str, base_url: &str) -> Option<String> {
    let base_origin = if let Ok(parsed) = reqwest::Url::parse(base_url) {
        format!(
            "{}://{}",
            parsed.scheme(),
            parsed.host_str().unwrap_or_default()
        )
    } else {
        String::new()
    };

    let mut search_idx = 0;
    while let Some(href_idx) = html[search_idx..].find("href=\"") {
        let abs_idx = search_idx + href_idx + 6;
        if let Some(end_quote) = html[abs_idx..].find('"') {
            let candidate = &html[abs_idx..abs_idx + end_quote];
            let lower = candidate.to_lowercase();
            if lower.ends_with(".zip") || lower.ends_with(".mcworld") {
                if candidate.starts_with("http://") || candidate.starts_with("https://") {
                    return Some(candidate.to_string());
                } else if candidate.starts_with('/') && !base_origin.is_empty() {
                    return Some(format!("{}{}", base_origin, candidate));
                }
            }
        }
        search_idx = abs_idx;
        if search_idx >= html.len() {
            break;
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_google_drive_id() {
        let url1 = "https://drive.google.com/file/d/1BZI09GgC29-abcd_1234/view?usp=sharing";
        assert_eq!(
            extract_google_drive_id(url1),
            Some("1BZI09GgC29-abcd_1234".to_string())
        );

        let url2 = "https://drive.google.com/open?id=xyz789";
        assert_eq!(extract_google_drive_id(url2), Some("xyz789".to_string()));
    }

    #[test]
    fn test_extract_mediafire_download_link() {
        let html = r#"<div><a id="downloadButton" href="https://download1234.mediafire.com/abcd/SkyBlock.zip" class="btn">Download</a></div>"#;
        assert_eq!(
            extract_mediafire_download_link(html),
            Some("https://download1234.mediafire.com/abcd/SkyBlock.zip".to_string())
        );
    }

    #[tokio::test]
    async fn test_resolve_direct_archive() {
        let url = "https://example.com/worlds/my_map.zip";
        let resolved = resolve_map_download_url(url).await.unwrap();
        assert_eq!(resolved, url);
    }

    #[tokio::test]
    async fn test_resolve_dropbox() {
        let url = "https://www.dropbox.com/s/abcdef/map.zip?dl=0";
        let resolved = resolve_map_download_url(url).await.unwrap();
        assert!(resolved.contains("dl=1"));
    }
}
