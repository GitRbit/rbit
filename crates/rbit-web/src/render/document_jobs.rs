//! Admission for rendering untrusted Markdown on request paths (file views, READMEs and
//! previews): large documents render on the blocking pool, at most [`MAX_JOBS`] at once,
//! so a burst of expensive documents never occupies the runtime's workers. A document
//! that finds every job taken is shown as escaped plain text at once instead of queueing;
//! nothing is cached for it, so the next view renders it properly.
//!
//! A job's permit lives in the blocking task itself, so a request that is cancelled or
//! times out does not release capacity while its rendering is still running.

use std::sync::Arc;

use tokio::sync::Semaphore;

use super::{
    document_cache::DocumentCache,
    markdown::{escape_html, render_markdown},
};
use crate::{
    error::{WebError, WebResult},
    urls::DocumentBase,
};

/// Renderings admitted at once.
pub const MAX_JOBS: usize = 4;

/// Sources this small render inline: cheaper than a hand-off.
pub const INLINE_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone)]
pub struct DocumentJobs {
    permits: Arc<Semaphore>,
}

impl Default for DocumentJobs {
    fn default() -> Self {
        Self::new(MAX_JOBS)
    }
}

impl DocumentJobs {
    #[must_use]
    pub fn new(jobs: usize) -> Self {
        Self { permits: Arc::new(Semaphore::new(jobs)) }
    }

    /// A repository document ([`render_markdown`] with `base`): a cache hit at once,
    /// otherwise an admitted rendering or plain text.
    pub async fn document(&self, source: String, base: DocumentBase) -> WebResult<String> {
        if let Some(html) = DocumentCache::shared().cached(&source, &base) {
            return Ok(html);
        }
        self.render(source, move |source| render_markdown(source, Some(&base))).await
    }

    /// `render(source)` inline for small sources, on the blocking pool when admitted, and
    /// as escaped plain text when every job is taken.
    pub async fn render<F>(&self, source: String, render: F) -> WebResult<String>
    where
        F: FnOnce(&str) -> String + Send + 'static,
    {
        if source.len() <= INLINE_BYTES {
            return Ok(render(&source));
        }
        let Ok(permit) = Arc::clone(&self.permits).try_acquire_owned() else {
            return Ok(plain(&source));
        };
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            render(&source)
        })
        .await
        .map_err(WebError::internal)
    }
}

/// A document shown as its source.
fn plain(source: &str) -> String {
    format!("<pre>{}</pre>", escape_html(source))
}

#[cfg(test)]
mod tests {
    use std::{sync::mpsc, time::Duration};

    use super::*;

    #[tokio::test]
    async fn excess_renderings_are_plain_and_cancelled_ones_keep_their_permit() {
        let jobs = DocumentJobs::new(1);
        let large = "*text* ".repeat(INLINE_BYTES);
        let (started, start) = tokio::sync::oneshot::channel();
        let (release, blocked) = mpsc::channel::<()>();
        let first = {
            let (jobs, large) = (jobs.clone(), large.clone());
            tokio::spawn(async move {
                jobs.render(large, move |_| {
                    started.send(()).unwrap();
                    blocked.recv_timeout(Duration::from_secs(10)).unwrap();
                    "<p>rendered</p>".to_owned()
                })
                .await
            })
        };
        start.await.unwrap();
        first.abort();
        assert!(first.await.unwrap_err().is_cancelled());
        let excess = jobs.render(large.clone(), |_| panic!("rendered beyond admission")).await.unwrap();
        assert!(excess.starts_with("<pre>*text* "), "the cancelled request's rendering still holds its permit");
        assert_eq!(jobs.render("*small*".into(), |source| format!("inline {source}")).await.unwrap(), "inline *small*");
        release.send(()).unwrap();
        let rendered = loop {
            let html = jobs.render(large.clone(), |_| "<p>again</p>".to_owned()).await.unwrap();
            if !html.starts_with("<pre>") {
                break html;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        };
        assert_eq!(rendered, "<p>again</p>", "capacity returns when the rendering ends");
    }

    #[test]
    fn plain_text_is_escaped() {
        assert_eq!(plain("<script>&"), "<pre>&lt;script&gt;&amp;</pre>");
    }
}
